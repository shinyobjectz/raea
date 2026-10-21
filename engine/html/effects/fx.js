/* raea effects runtime (fx.js).
 *
 * One line per effect on any element:   <div data-fx="glow(16px, #7cf) grain(0.2)">
 * expands into a standard SVG <filter> (one per distinct data-fx value) and `filter: url(#id)` on the
 * element, so Chrome (the HTML composition format), WebKit (the apps' preview) and raea render the same markup.
 * Effects that change over time (grain, warp with speed) update their filter on the composition format's `hf-seek`
 * event, so a frame depends only on its time.
 *
 * The effect list and their one-line forms are in effects/catalog.json; docs/ENGINE-COMPLETENESS.md
 * (effects). Keep every expansion deterministic: no Math.random, no wall clock.
 */
(function () {
  "use strict";
  if (window.__uvFx) return;
  var NS = "http://www.w3.org/2000/svg";

  // ---- argument parsing -------------------------------------------------------------------------
  // "glow(16px, #7cf) grain(0.2)" -> [{name: "glow", args: ["16px", "#7cf"]}, {name: "grain", args: ["0.2"]}]
  function parse(spec) {
    var out = [], re = /([a-z][a-z0-9-]*)\s*(?:\(([^()]*(?:\([^()]*\)[^()]*)*)\))?/gi, m;
    while ((m = re.exec(spec))) {
      var args = [];
      if (m[2] !== undefined && m[2].trim() !== "") {
        // split on commas outside parentheses (rgb(…) colours keep theirs)
        var depth = 0, cur = "";
        for (var i = 0; i < m[2].length; i++) {
          var c = m[2][i];
          if (c === "(") depth++;
          if (c === ")") depth--;
          if (c === "," && depth === 0) { args.push(cur.trim()); cur = ""; } else cur += c;
        }
        args.push(cur.trim());
      }
      out.push({ name: m[1].toLowerCase(), args: args });
    }
    return out;
  }
  function px(v, d) {
    if (v === undefined || v === "") return d;
    var n = parseFloat(v);
    return isFinite(n) ? n : d;
  }
  function num(v, d) { return px(v, d); }
  // a time in seconds: "0.2s", "200ms" or a bare number of seconds
  function sec(v, d) {
    if (v === undefined || v === "") return d;
    var n = parseFloat(v);
    if (!isFinite(n)) return d;
    return /ms\s*$/i.test(v) ? n / 1000 : n;
  }
  // a list of times ("0.15s 0.55s 1.05s"), as seconds
  function times(v) {
    return v ? String(v).trim().split(/[\s,]+/).map(function (x) { return sec(x, NaN); }).filter(isFinite) : [];
  }
  function clamp01(x) { return Math.max(0, Math.min(1, x)); }
  function smooth(x) { x = clamp01(x); return x * x * (3 - 2 * x); }
  // 0..1 over one period: "triangle" peaks mid-period, "saw" rises through it, "square" is on for its first half
  function wave(t, period, shape) {
    var f = t / period - Math.floor(t / period);
    if (shape === "saw") return f;
    if (shape === "square") return f < 0.5 ? 1 : 0;
    return 1 - Math.abs(2 * f - 1);
  }
  function color(v, d) { return v === undefined || v === "" ? d : v; }

  // ---- presets ------------------------------------------------------------------------------------
  // Each returns {pad: user-space px the effect reaches past the box, body: primitive markup, seek?: fn(t),
  // xf?: fn(t, el) -> {x, y, rot, sx, sy}}. Primitives read `in` = the previous preset's result
  // ("SourceGraphic" for the first) and end in a result named `out`. `xf` moves the element itself with
  // the individual transform properties (translate / rotate / scale), which compose with any `transform`
  // the composition animates (their arguments don't take ranges). seek and xf get the composition's
  // time; span(el) has the element's.
  var PRESETS = {
    // a soft halo in `color` (default: the content's own colours) around shapes and text
    glow: function (a, inp, out) {
      var r = px(a[0], 12), c = a[1], s = num(a[2], 1), sd = r / 2;
      var halo = c
        ? '<feGaussianBlur in="' + inp + '" stdDeviation="' + sd + '" result="' + out + 'b"/>' +
          '<feFlood flood-color="' + c + '" flood-opacity="' + s + '" result="' + out + 'c"/>' +
          '<feComposite in="' + out + 'c" in2="' + out + 'b" operator="in" result="' + out + 'h"/>'
        : '<feGaussianBlur in="' + inp + '" stdDeviation="' + sd + '" result="' + out + 'h"/>';
      return {
        pad: 3 * sd + 2,
        body: halo + '<feMerge result="' + out + '"><feMergeNode in="' + out + 'h"/><feMergeNode in="' + out +
          'h"/><feMergeNode in="' + inp + '"/></feMerge>',
      };
    },
    // shapes melt together where they meet (blur, then an alpha threshold), keeping their colours
    goo: function (a, inp, out) {
      var r = px(a[0], 10);
      return {
        pad: 3 * r + 2,
        body: '<feGaussianBlur in="' + inp + '" stdDeviation="' + r + '" result="' + out + 'b"/>' +
          '<feColorMatrix in="' + out + 'b" values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -9" result="' + out + 'g"/>' +
          '<feComposite in="' + inp + '" in2="' + out + 'g" operator="atop" result="' + out + 's"/>' +
          '<feMerge result="' + out + '"><feMergeNode in="' + out + 'g"/><feMergeNode in="' + out + 's"/></feMerge>',
      };
    },
    // colour grade: grade(saturation, brightness, contrast) — 1 = unchanged; grade(0) is black and white
    grade: function (a, inp, out) {
      var sat = Math.max(0, num(a[0], 1)), br = Math.max(0, num(a[1], 1)), ct = Math.max(0, num(a[2], 1));
      // contrast pivots on mid grey after the brightness gain
      var slope = round4(br * ct), icpt = round4(0.5 - 0.5 * ct);
      var f = function (c) { return '<feFunc' + c + ' type="linear" slope="' + slope + '" intercept="' + icpt + '"/>'; };
      return {
        pad: 0,
        body: '<feColorMatrix in="' + inp + '" type="saturate" values="' + round4(sat) + '" result="' + out + 's"/>' +
          '<feComponentTransfer in="' + out + 's" result="' + out + '">' + f("R") + f("G") + f("B") + '</feComponentTransfer>',
      };
    },
    // two-colour look: shadows -> dark, highlights -> light
    duotone: function (a, inp, out) {
      var d = rgb(color(a[0], "#1a1033")), l = rgb(color(a[1], "#ffcc33"));
      return {
        pad: 0,
        body: '<feColorMatrix in="' + inp + '" values="0.2126 0.7152 0.0722 0 0  0.2126 0.7152 0.0722 0 0  ' +
          '0.2126 0.7152 0.0722 0 0  0 0 0 1 0" result="' + out + 'l"/>' +
          '<feComponentTransfer in="' + out + 'l" result="' + out + '">' +
          '<feFuncR type="table" tableValues="' + d[0] + " " + l[0] + '"/>' +
          '<feFuncG type="table" tableValues="' + d[1] + " " + l[1] + '"/>' +
          '<feFuncB type="table" tableValues="' + d[2] + " " + l[2] + '"/></feComponentTransfer>',
      };
    },
    // fine moving film grain over the picture (amount 0-1); re-seeded every frame
    grain: function (a, inp, out, id) {
      var amt = Math.max(0, Math.min(1, num(a[0], 0.25))), size = num(a[1], 1);
      var tid = id + "-" + out + "t";
      if ((a[2] || "").trim() === "film") {
        // film grain: grey noise added to the picture, stronger in the shadows (out = in + k(n - 0.5)(1 - 0.55 in),
        // one arithmetic composite), as a consumer editor's Grain measures; the noise's alpha is held at 1. Like a consumer editor's, the
        // grain stands still (a fixed texture over the clip)
        var k = round4(amt);
        return {
          pad: 0,
          body: '<feTurbulence id="' + tid + '" type="fractalNoise" baseFrequency="' + round4(0.9 / size) +
            '" numOctaves="1" seed="0" stitchTiles="stitch" result="' + out + 'n"/>' +
            '<feColorMatrix in="' + out + 'n" type="matrix" values="0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 ' +
            '0.2126 0.7152 0.0722 0 0 0 0 0 0 1" result="' + out + 'm"/>' +
            '<feComposite in="' + inp + '" in2="' + out + 'm" operator="arithmetic" k1="' + round4(-0.55 * k) + '" k2="' +
            round4(1 + 0.275 * k) + '" k3="' + k + '" k4="' + round4(-0.5 * k) + '" result="' + out + 'o"/>' +
            '<feComposite in="' + out + 'o" in2="' + inp + '" operator="in" result="' + out + '"/>',
        };
      }
      return {
        pad: 0,
        body: '<feTurbulence id="' + tid + '" type="fractalNoise" baseFrequency="' + (0.9 / size) +
          '" numOctaves="2" seed="0" stitchTiles="stitch" result="' + out + 'n"/>' +
          '<feColorMatrix in="' + out + 'n" type="saturate" values="0" result="' + out + 'm"/>' +
          '<feComponentTransfer in="' + out + 'm" result="' + out + 'a"><feFuncA type="linear" slope="' + amt +
          '"/></feComponentTransfer>' +
          '<feBlend in="' + out + 'a" in2="' + inp + '" mode="overlay" result="' + out + 'o"/>' +
          '<feComposite in="' + out + 'o" in2="' + inp + '" operator="in" result="' + out + '"/>',
        seek: function (t) { setAttr(tid, "seed", String(Math.floor(t * 24) % 997)); },
      };
    },
    // wavy distortion; speed > 0 makes it flow over time
    warp: function (a, inp, out, id) {
      var amt = px(a[0], 20), scale = num(a[1], 0.02), speed = num(a[2], 0);
      var tid = id + "-" + out + "t";
      return {
        pad: amt / 2 + 2,
        body: '<feTurbulence id="' + tid + '" type="turbulence" baseFrequency="' + scale + " " + scale * 2 +
          '" numOctaves="2" seed="3" result="' + out + 'n"/>' +
          '<feDisplacementMap in="' + inp + '" in2="' + out + 'n" scale="' + amt +
          '" xChannelSelector="R" yChannelSelector="G" result="' + out + '"/>',
        seek: speed > 0 ? function (t) { setAttr(tid, "seed", String(3 + Math.floor(t * speed * 10))); } : null,
      };
    },
    // background blur behind the subject of a clip's matte track: background-blur(person, 16px)
    "background-blur": function (a, inp, out, id, el) {
      var r = px(a[1], 16), m = matte(el, a[0] || "person", id, out);
      if (!m) return { pad: 0, body: '<feOffset in="' + inp + '" result="' + out + '"/>' };
      return {
        pad: 0,
        body: '<feGaussianBlur in="' + inp + '" stdDeviation="' + r / 2 + '" edgeMode="duplicate" result="' + out + 'b"/>' +
          m.image + '<feComposite in="' + inp + '" in2="' + out + 'm" operator="in" result="' + out + 's"/>' +
          '<feMerge result="' + out + '"><feMergeNode in="' + out + 'b"/><feMergeNode in="' + out + 's"/></feMerge>',
        seek: m.seek,
      };
    },
    // only the subject of a clip's matte track; the rest is transparent: cutout(person)
    cutout: function (a, inp, out, id, el) {
      var m = matte(el, a[0] || "person", id, out);
      if (!m) return { pad: 0, body: '<feOffset in="' + inp + '" result="' + out + '"/>' };
      return {
        pad: 0,
        body: m.image + '<feComposite in="' + inp + '" in2="' + out + 'm" operator="in" result="' + out + '"/>',
        seek: m.seek,
      };
    },
    // gaussian blur; the radius is the standard deviation, like CSS filter: blur(). blur(12px, linear)
    // mixes in linear light (what cameras and video editors do: highlights spread brighter)
    blur: function (a, inp, out) {
      var r = Math.max(0, px(a[0], 8));
      return { pad: 3 * r + 2, body: '<feGaussianBlur in="' + inp + '" stdDeviation="' + r + '"' + light(a[1]) + ' result="' + out + '"/>' };
    },
    // blur along one axis: motion-blur(24px) is horizontal, motion-blur(24px, vertical); `sharp` (0-1)
    // keeps that much of the unblurred picture over the streak; `linear` blurs in linear light
    "motion-blur": function (a, inp, out) {
      var l = px(a[0], 20), v = (a[1] || "").trim() === "vertical", sd = l / 2, k = clamp01(num(a[2], 0));
      var blur = '<feGaussianBlur in="' + inp + '" stdDeviation="' + (v ? "0 " + sd : sd + " 0") + '"' + light(a[3]) +
        ' result="' + (k ? out + "b" : out) + '"/>';
      return {
        pad: 3 * sd + 2,
        body: k ? blur + '<feComposite in="' + inp + '" in2="' + out + 'b" operator="arithmetic" k1="0" k2="' + k +
          '" k3="' + round4(1 - k) + '" k4="0" result="' + out + '"/>' : blur,
      };
    },
    // the picture flipped: mirror() left-right, mirror(vertical) upside down
    mirror: function (a, inp, out) {
      var v = (a[0] || "").trim() === "vertical";
      return { pad: 0, body: pass(inp, out), xf: function () { return v ? { sy: -1 } : { sx: -1 }; } };
    },
    // the element scaled about its centre: scale(1.2); with keyframes, scale(1..1.5..1.5..1@0.4s..0.9s..2s..2.6s)
    scale: function (a, inp, out) {
      var sx = num(a[0], 1), sy = num(a[1], sx);
      return { pad: 0, body: pass(inp, out), xf: function () { return { sx: sx, sy: sy }; } };
    },
    // the element turned about its centre, in degrees: rotate(8), rotate(-4..4)
    rotate: function (a, inp, out) {
      var r = num(a[0], 0);
      return { pad: 0, body: pass(inp, out), xf: function () { return { rot: r }; } };
    },
    // push in: zoom(1.5) scales up over `in` (default 0.6s, easing out) and holds; with `out` it snaps
    // back over the element's last `out` seconds: accelerating to full size in the first 42%, then a
    // slight landing dip (3% of the zoom) that settles by the end
    zoom: function (a, inp, out) {
      var z = num(a[0], 1.3), tin = sec(a[1], 0.6), tout = sec(a[2], 0);
      return {
        pad: 0,
        body: pass(inp, out),
        xf: function (t, el) {
          var sp = span(el), u = t - sp.start, k = tin > 0 ? Math.sin(Math.PI / 2 * clamp01(u / tin)) : 1;
          var s = 1 + (z - 1) * k, v = tout > 0 ? (u - (sp.dur - tout)) / tout : -1;
          if (v >= 0.42) s = 1 - 0.033 * (z - 1) * Math.sin(Math.PI * clamp01((v - 0.42) / 0.58));
          else if (v >= 0) s = s - (s - 1) * Math.pow(v / 0.42, 2);
          return { sx: s, sy: s };
        },
      };
    },
    // camera shake: shake(2) jolts the element by 2% of its size to a new corner every `step` (default
    // 0.05s), round a square and back through the middle, so it rests every 5 steps
    shake: function (a, inp, out) {
      var amt = num(a[0], 2) / 100, step = Math.max(1 / 120, sec(a[1], 0.05)), z = num(a[2], 1);
      var path = [[1, -1], [1, 1], [-1, 1], [-1, -1], [0, 0], [-1, -1], [-1, 1], [1, 1], [1, -1], [0, 0]];
      return {
        pad: 0,
        body: pass(inp, out),
        xf: function (t, el) {
          var k = Math.floor((t - span(el).start) / step - 0.25), c = path[((k % 10) + 10) % 10];
          return { x: c[0] * amt * (el.offsetWidth || 0), y: c[1] * amt * (el.offsetHeight || 0), sx: z, sy: z };
        },
      };
    },
    // a colour washing over the picture and away, every `period`: flash(#fff, 0.2s); peak 0-1;
    // shape triangle (in and out) | saw (builds, then cuts) | square (on / off). With `at`, one flash of
    // `period` at each of those times on the element's clock instead: flash(#fff, 0.12s, 1, triangle, 0.15s 0.55s)
    flash: function (a, inp, out, id, el) {
      var c = color(a[0], "#fff"), per = Math.max(1 / 120, sec(a[1], 0.2)), peak = clamp01(num(a[2], 1)),
        shape = (a[3] || "triangle").trim(), beats = times(a[4]), fid = id + "-" + out + "f";
      var level = function (t) {
        if (!beats.length) return wave(t, per, shape);
        var u = t - span(el).start, v = 0;
        for (var i = 0; i < beats.length; i++) {
          var d = u - beats[i];
          if (d >= 0 && d < per) v = Math.max(v, wave(d, per, shape));
        }
        return v;
      };
      return {
        pad: 0,
        body: '<feFlood id="' + fid + '" flood-color="' + c + '" flood-opacity="0" result="' + out + 'f"/>' +
          '<feComposite in="' + out + 'f" in2="' + inp + '" operator="atop" result="' + out + '"/>',
        seek: function (t) { setAttr(fid, "flood-opacity", String(round4(peak * level(t)))); },
      };
    },
    // chromatic aberration: rgb-split(6px) pulls red one way and blue the other (angle in degrees,
    // 0 = horizontal); keyframes animate it: rgb-split(0px..12px)
    "rgb-split": function (a, inp, out) {
      var d = px(a[0], 6), ang = num(a[1], 0) * Math.PI / 180;
      var dx = round4(d * Math.cos(ang)), dy = round4(d * Math.sin(ang));
      var only = function (c) {
        var m = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0];
        m[c * 6] = 1;
        return m.join(" ");
      };
      return {
        pad: Math.abs(d) + 2,
        body: '<feColorMatrix in="' + inp + '" type="matrix" values="' + only(0) + '" result="' + out + 'r"/>' +
          '<feOffset in="' + out + 'r" dx="' + dx + '" dy="' + dy + '" result="' + out + 'ro"/>' +
          '<feColorMatrix in="' + inp + '" type="matrix" values="' + only(1) + '" result="' + out + 'g"/>' +
          '<feColorMatrix in="' + inp + '" type="matrix" values="' + only(2) + '" result="' + out + 'b"/>' +
          '<feOffset in="' + out + 'b" dx="' + (-dx) + '" dy="' + (-dy) + '" result="' + out + 'bo"/>' +
          '<feBlend in="' + out + 'ro" in2="' + out + 'g" mode="screen" result="' + out + 'rg"/>' +
          '<feBlend in="' + out + 'rg" in2="' + out + 'bo" mode="screen" result="' + out + '"/>',
      };
    },
    // the colours inverted, like a film negative: invert() or invert(0.5) half way. It can flicker:
    // invert(1, 0.08s) toggles every half of that period; invert(1, 0.08s, 1s, 0.4s) only for the first
    // 0.4s of every second
    invert: function (a, inp, out, id) {
      var amt = clamp01(num(a[0], 1)), fl = sec(a[1], 0), every = sec(a[2], 0), dur = sec(a[3], every / 2);
      var fid = id + "-" + out + "i", on = round4(amt) + " " + round4(1 - amt);
      var funcs = ["R", "G", "B"].map(function (c) {
        return '<feFunc' + c + ' id="' + fid + c + '" type="table" tableValues="' + on + '"/>';
      }).join("");
      return {
        pad: 0,
        body: '<feComponentTransfer in="' + inp + '" result="' + out + '">' + funcs + '</feComponentTransfer>',
        seek: fl > 0 || every > 0 ? function (t) {
          var lit = (every > 0 ? (t / every - Math.floor(t / every)) * every < dur : true) &&
            (fl > 0 ? t / fl - Math.floor(t / fl) >= 0.5 : true);
          ["R", "G", "B"].forEach(function (c) { setAttr(fid + c, "tableValues", lit ? on : "0 1"); });
        } : null,
      };
    },
    // iris reveal: iris(0.3s, 2.6s) fades the picture out over the first 0.3s, then opens it again from
    // the middle through an oval that covers it by 2.6s; soft = edge width, aspect = the oval's width
    // against the box's proportions (below 1: taller)
    iris: function (a, inp, out, id, el) {
      var t0 = sec(a[0], 0.3), t1 = Math.max(t0 + 0.01, sec(a[1], 2.6)), soft = Math.max(0.01, num(a[2], 0.15)),
        asp = Math.max(0.1, num(a[3], 1));
      var l = layer(el, id, out, function (g, w, h, t) {
        var u = t - span(el).start;
        var fade = t0 > 0 ? 1 - smooth(u / t0) : 0;
        // normalised radius (1 = the box's half size), past the corners at the end
        var r = Math.max(0, (u - t0) / (t1 - t0)) * (Math.SQRT2 + soft) / Math.min(1, asp);
        if (fade > 0) {
          g.fillStyle = "rgba(255,255,255," + round4(fade) + ")";
          g.fillRect(0, 0, w, h);
        }
        if (r > 0) {
          g.save();
          g.translate(w / 2, h / 2);
          g.scale(w / 2 * asp, h / 2);
          var grad = g.createRadialGradient(0, 0, Math.max(0, r - soft), 0, 0, r + soft);
          grad.addColorStop(0, "rgba(255,255,255,1)");
          grad.addColorStop(1, "rgba(255,255,255,0)");
          g.fillStyle = grad;
          g.fillRect(-1 / asp, -1, 2 / asp, 2);
          g.restore();
        }
      }, true);
      if (!l) return { pad: 0, body: pass(inp, out) };
      return {
        pad: 0,
        body: l.image + '<feComposite in="' + inp + '" in2="' + out + 'L" operator="in" result="' + out + '"/>',
        seek: l.seek,
      };
    },
    // darkened edges: vignette(0.6) — amount 0-1, size = the clear middle as a fraction of the box,
    // softness = how far the fall-off runs
    vignette: function (a, inp, out, id, el) {
      var amt = clamp01(num(a[0], 0.5)), size = clamp01(num(a[1], 0.5)), soft = Math.max(0.01, num(a[2], 0.5)),
        c = rgb(color(a[3], "#000")).map(function (v) { return Math.round(v * 255); });
      var l = layer(el, id, out, function (g, w, h) {
        // an ellipse with the box's proportions: draw a circle into a square and stretch it
        g.save();
        g.scale(w / h, 1);
        var cx = h / 2, cy = h / 2, r = Math.sqrt(0.5) * h;
        var grad = g.createRadialGradient(cx, cy, r * size, cx, cy, r * Math.min(2, size + soft));
        var rgba = "rgba(" + c[0] + "," + c[1] + "," + c[2] + ",";
        grad.addColorStop(0, rgba + "0)");
        grad.addColorStop(1, rgba + amt + ")");
        g.fillStyle = grad;
        g.fillRect(0, 0, h, h);
        g.restore();
      });
      if (!l) return { pad: 0, body: pass(inp, out) };
      return { pad: 0, body: l.image + '<feComposite in="' + out + 'L" in2="' + inp + '" operator="atop" result="' + out + '"/>' };
    },
    // light bleeding out of the bright parts: bloom(24px, 0.6) — radius, strength, threshold (0-1: how
    // bright a part has to be to bloom)
    bloom: function (a, inp, out) {
      var r = px(a[0], 20), k = num(a[1], 0.6), th = clamp01(num(a[2], 0.6)), sd = r / 2;
      var slope = round4(1 / Math.max(0.01, 1 - th)), icpt = round4(-th / Math.max(0.01, 1 - th));
      return {
        pad: 3 * sd + 2,
        body: '<feComponentTransfer in="' + inp + '" result="' + out + 'h">' +
          '<feFuncR type="linear" slope="' + slope + '" intercept="' + icpt + '"/>' +
          '<feFuncG type="linear" slope="' + slope + '" intercept="' + icpt + '"/>' +
          '<feFuncB type="linear" slope="' + slope + '" intercept="' + icpt + '"/></feComponentTransfer>' +
          '<feGaussianBlur in="' + out + 'h" stdDeviation="' + sd + '" result="' + out + 'b"/>' +
          '<feComposite in="' + inp + '" in2="' + out + 'b" operator="arithmetic" k1="0" k2="1" k3="' + k +
          '" k4="0" result="' + out + '"/>',
      };
    },
  };

  function rgb(c) {
    var m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(c.trim());
    if (m) {
      var h = m[1].length === 3 ? m[1].replace(/./g, "$&$&") : m[1];
      return [0, 2, 4].map(function (i) { return round4(parseInt(h.substr(i, 2), 16) / 255); });
    }
    m = /^rgba?\(([^)]*)\)$/i.exec(c.trim());
    if (m) return m[1].split(/[ ,/]+/).slice(0, 3).map(function (v) { return round4(parseFloat(v) / 255); });
    return [0, 0, 0];
  }
  function round4(x) { return Math.round(x * 10000) / 10000; }
  // a blur's colour space: "linear" = linear light, else sRGB (the filter's own)
  function light(v) { return (v || "").trim() === "linear" ? ' color-interpolation-filters="linearRGB"' : ""; }
  function pass(inp, out) { return '<feOffset in="' + inp + '" result="' + out + '"/>'; }

  // ---- layers ---------------------------------------------------------------------------------------
  // A picture drawn with canvas 2D over the element's box (vignettes, light, particles), as an feImage
  // (result "<out>L") holding the canvas's PNG. draw(g, w, h, t) paints it; with `dynamic` it is
  // redrawn for every frame's time.
  function layer(el, id, out, draw, dynamic) {
    var w = Math.round(el.offsetWidth || 0), h = Math.round(el.offsetHeight || 0);
    if (!w || !h) return null;
    var lid = id + "-" + out + "L", c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    function url(t) {
      var g = c.getContext("2d");
      g.clearRect(0, 0, w, h);
      draw(g, w, h, t);
      return c.toDataURL("image/png");
    }
    return {
      image: '<feImage id="' + lid + '" href="' + url(0) + '" x="0" y="0" width="' + w + '" height="' + h +
        '" preserveAspectRatio="none" result="' + out + 'L"/>',
      seek: dynamic ? function (t) { setAttr(lid, "href", url(t)); } : null,
    };
  }

  // ---- tracks (docs/ML-TRACKS.md) ---------------------------------------------------------------------
  // tracks/<key>/<kind>/ next to the composition, key = the clip's project path with "/" as "-" and no
  // leading "assets/"; frame i covers clip time start + i/fps.
  var trackInfo = {};
  function trackDir(el, kind) {
    // data-fx-track names the clip's track folder (tracks/<key>), relative to the composition like a src
    var named = el.getAttribute("data-fx-track");
    if (named) return named.replace(/\/+$/, "") + "/" + kind + "/";
    // otherwise tracks/<key> beside the composition; raea swaps <video> for an
    // <img data-uv-src>, where a source may be absolute
    var src = el.getAttribute("src") || el.getAttribute("data-uv-src") ||
      (el.querySelector && el.querySelector("source") ? el.querySelector("source").getAttribute("src") : "") || "";
    try {
      var base = String(location.href).replace(/[?#].*$/, "").replace(/[^\/]*$/, "");
      [base, base.replace(/^file:\/\//, "")].forEach(function (b) { if (b && src.indexOf(b) === 0) src = src.slice(b.length); });
    } catch (e) {}
    var key = src.replace(/^\.\//, "").replace(/^assets\//, "").replace(/\//g, "-");
    return key ? "tracks/" + key + "/" + kind + "/" : null;
  }
  function readTrack(dir) {
    if (trackInfo[dir] !== undefined) return trackInfo[dir];
    var info = null;
    try {
      var x = new XMLHttpRequest();
      x.open("GET", dir + "track.json", false);
      x.send(null);
      if ((x.status === 200 || x.status === 0) && x.responseText) info = JSON.parse(x.responseText);
    } catch (e) { info = null; }
    trackInfo[dir] = info;
    return info;
  }
  function pad6(n) { var s = String(n); while (s.length < 6) s = "0" + s; return s; }
  // A matte track as an feImage stretched over the element's box (result "<out>m"), and the seek
  // function that shows the frame for the clip's current time.
  function matte(el, kind, id, out) {
    var dir = trackDir(el, kind);
    var info = dir && readTrack(dir);
    if (!info || !info.frames) {
      if (window.console) console.warn("data-fx: no " + kind + " track for " + (el.getAttribute("src") || "this element"));
      return null;
    }
    var w = el.offsetWidth || 0, h = el.offsetHeight || 0;
    var iid = id + "-" + out + "m";
    var start = parseFloat(el.getAttribute("data-start")) || 0;
    var mediaStart = parseFloat(el.getAttribute("data-playback-start") || el.getAttribute("data-media-start")) || 0;
    var rate = parseFloat(el.getAttribute("data-playback-rate")) || 1;
    var fps = info.fps || 30, first = info.start || 0, last = info.frames - 1;
    function frameAt(t) {
      var media = mediaStart + Math.max(0, t - start) * rate;
      return Math.max(0, Math.min(last, Math.floor((media - first) * fps + 1e-6)));
    }
    return {
      image: '<feImage id="' + iid + '" href="' + dir + pad6(frameAt(0)) + '.png" x="0" y="0" width="' + w +
        '" height="' + h + '" preserveAspectRatio="none" result="' + out + 'm"/>',
      seek: function (t) { setAttr(iid, "href", dir + pad6(frameAt(t)) + ".png"); },
    };
  }
  function setAttr(id, k, v) { var e = document.getElementById(id); if (e) e.setAttribute(k, v); }

  // ---- expansion ------------------------------------------------------------------------------------
  var defs = null, made = {}, seekers = [];
  function host() {
    if (!defs) {
      defs = document.createElementNS(NS, "svg");
      defs.setAttribute("width", "0");
      defs.setAttribute("height", "0");
      defs.setAttribute("aria-hidden", "true");
      defs.setAttribute("style", "position:absolute;width:0;height:0;overflow:hidden");
      document.body.appendChild(defs);
    }
    return defs;
  }
  function idFor(spec) {
    var h = 5381;
    for (var i = 0; i < spec.length; i++) h = ((h * 33) ^ spec.charCodeAt(i)) >>> 0;
    return "uvfx-" + h.toString(36);
  }

  // An argument "a..b" (e.g. blur(0px..12px)) runs from a to b over the element's time on screen
  // (its data-start / data-duration, else its composition's); "a..b@0.3s..1.1s" runs between those
  // times of the element's own (before: a, after: b). More stops make keyframes:
  // "1..1.5..1.5..1@0.4s..0.9s..2s..2.6s" (without times they spread evenly). Linear between stops.
  // u = the element's time, dur = its duration.
  var NUM = /^\s*(-?\d*\.?\d+)([a-z%]*)\s*$/i;
  function argAt(a, u, dur) {
    if (a.indexOf("..") < 0) return a;
    var at = a.split("@"), vals = at[0].split(".."), unit = "", v = [];
    for (var i = 0; i < vals.length; i++) {
      var m = NUM.exec(vals[i]);
      if (!m) return a;
      v.push(+m[1]);
      unit = unit || m[2];
    }
    var ts = at.length > 1 ? at[1].split("..").map(function (x) { return sec(x.trim(), NaN); }) : null;
    if (!ts || ts.length !== v.length || ts.some(function (x) { return !isFinite(x); })) {
      ts = v.map(function (_, k) { return v.length > 1 ? dur * k / (v.length - 1) : 0; });
    }
    var out = v[v.length - 1];
    if (u <= ts[0]) out = v[0];
    else {
      for (var k = 1; k < v.length; k++) {
        if (u <= ts[k]) {
          var q = ts[k] > ts[k - 1] ? (u - ts[k - 1]) / (ts[k] - ts[k - 1]) : 1;
          out = v[k - 1] + (v[k] - v[k - 1]) * q;
          break;
        }
      }
    }
    return String(Math.round(out * 10000) / 10000) + unit;
  }
  function span(el) {
    var start = parseFloat(el.getAttribute("data-start")) || 0, dur = parseFloat(el.getAttribute("data-duration"));
    for (var n = el; !(dur > 0) && n && n.getAttribute; n = n.parentNode) dur = parseFloat(n.getAttribute("data-duration"));
    return { start: start, dur: dur > 0 ? dur : 1 };
  }
  function build(fx, id, el, u, dur) {
    var inp = "SourceGraphic", out = { body: "", pad: 0, seeks: [], xfs: [], unknown: [] };
    for (var i = 0; i < fx.length; i++) {
      var p = PRESETS[fx[i].name];
      if (!p) { out.unknown.push(fx[i].name); continue; }
      var r = p(fx[i].args.map(function (a) { return argAt(a, u, dur); }), inp, "e" + i, id, el);
      out.body += r.body;
      out.pad += r.pad;
      if (r.seek) out.seeks.push(r.seek);
      if (r.xf) out.xfs.push(r.xf);
      inp = "e" + i;
    }
    return out;
  }

  function apply(el) {
    var spec = (el.getAttribute("data-fx") || "").trim();
    if (!spec) return;
    // one filter per effect line and clip (track effects read the element's own clip)
    var id = idFor(spec + "|" + (el.getAttribute("src") || ""));
    if (!made[id]) {
      var fx = parse(spec), ranged = /\.\./.test(spec), sp = span(el), first = build(fx, id, el, 0, sp.dur);
      var body = first.body, unknown = first.unknown;
      // a range's reach, sampled over the element's time (keyframes can peak in the middle)
      var pad = first.pad;
      if (ranged) {
        for (var k = 0; k <= 16; k++) pad = Math.max(pad, build(fx, id, el, sp.dur * k / 16, sp.dur).pad);
      }
      if (ranged) {
        // time-varying: rebuild the primitives for each frame's time
        var fid = id, last = body;
        seekers.push(function (t) {
          var b = build(fx, fid, el, t - sp.start, sp.dur);
          var f = document.getElementById(fid);
          if (f && b.body !== last) { f.innerHTML = b.body; last = b.body; }
          for (var j = 0; j < b.seeks.length; j++) b.seeks[j](t);
          move.xfs = b.xfs;
          if (b.xfs.length) move(t);
        });
        var move = mover(el, first.xfs);
      } else {
        for (var j = 0; j < first.seeks.length; j++) seekers.push(first.seeks[j]);
      }
      if (unknown.length && window.console) console.warn("data-fx: unknown effect " + unknown.join(", "));
      var f = document.createElementNS(NS, "filter");
      f.setAttribute("id", id);
      // sRGB like CSS filters; each preset is tuned in sRGB
      f.setAttribute("color-interpolation-filters", "sRGB");
      // the filter region: the element's box plus how far the effects reach (user space, CSS px)
      var w = el.offsetWidth || 0, h = el.offsetHeight || 0, p0 = Math.ceil(pad);
      var region = w && h
        ? { filterUnits: "userSpaceOnUse", x: -p0, y: -p0, width: w + 2 * p0, height: h + 2 * p0 }
        : { x: "-50%", y: "-50%", width: "200%", height: "200%" };
      for (var k in region) f.setAttribute(k, String(region[k]));
      f.innerHTML = body;
      host().appendChild(f);
      made[id] = { xfs: ranged ? [] : first.xfs };
    }
    if (made[id].xfs.length) seekers.push(mover(el, made[id].xfs));
    var own = el.getAttribute("data-fx-filter") || "";
    el.style.filter = "url(#" + id + ")" + (own ? " " + own : "");
  }

  // the presets' transforms on the element, through the individual transform properties: translations
  // and rotations add, scales multiply. A property is only written once a preset has used it.
  function mover(el, xfs) {
    var used = {};
    function put(k, v, idle) {
      if (v === idle && !used[k]) return;
      used[k] = true;
      el.style.setProperty(k, v === idle ? "none" : v);
    }
    var move = function (t) {
      var list = move.xfs || xfs, x = 0, y = 0, rot = 0, sx = 1, sy = 1;
      for (var i = 0; i < list.length; i++) {
        var m = list[i](t, el) || {};
        x += m.x || 0;
        y += m.y || 0;
        rot += m.rot || 0;
        if (m.sx !== undefined) sx *= m.sx;
        if (m.sy !== undefined) sy *= m.sy;
      }
      put("translate", x || y ? round4(x) + "px " + round4(y) + "px" : "", "");
      put("rotate", rot ? round4(rot) + "deg" : "", "");
      put("scale", sx !== 1 || sy !== 1 ? round4(sx) + " " + round4(sy) : "", "");
    };
    return move;
  }

  function expand(root) {
    var els = (root || document).querySelectorAll("[data-fx]");
    for (var i = 0; i < els.length; i++) apply(els[i]);
  }
  function seek(t) { for (var i = 0; i < seekers.length; i++) seekers[i](t); }

  window.__uvFx = { expand: expand, seek: seek, parse: parse, presets: Object.keys(PRESETS) };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", function () { expand(); seek(0); });
  else { expand(); seek(0); }
  window.addEventListener("hf-seek", function (e) { seek(e && e.detail ? +e.detail.time || 0 : 0); });
})();
