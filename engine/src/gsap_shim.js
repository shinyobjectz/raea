// Portions ported from HyperFrames (Apache-2.0, https://github.com/heygen-com/hyperframes); see NOTICE.
// GSAP-compatible authoring shim for raea-blitz.
//
// Compositions call the GSAP API as usual (gsap.timeline(), tl.to/from/fromTo/set, stagger,
// labels, position parameters, keyframes...). Nothing is animated here: this file only resolves
// the authored structure the way GSAP does at call time (positions, durations, staggers,
// function-based values, base values from getComputedStyle) and hands the resulting tree to Rust,
// where animato evaluates every tween per frame. See src/gsap.rs.
(function () {
  "use strict";
  var g = globalThis;
  if (g.__hfb) return;

  var RESERVED = {
    duration: 1, ease: 1, delay: 1, repeat: 1, yoyo: 1, repeatDelay: 1, stagger: 1, overwrite: 1,
    immediateRender: 1, lazy: 1, paused: 1, id: 1, runBackwards: 1, startAt: 1, keyframes: 1,
    inherit: 1, callbackScope: 1, yoyoEase: 1, data: 1, defaults: 1, reversed: 1, smoothChildTiming: 1,
    autoRemoveChildren: 1, onStart: 1, onUpdate: 1, onComplete: 1, onRepeat: 1, onReverseComplete: 1,
    onInterrupt: 1, onStartParams: 1, onUpdateParams: 1, onCompleteParams: 1, onRepeatParams: 1,
    onReverseCompleteParams: 1, scrollTrigger: 1, easeEach: 1, transformOrigin: 0, timeScale: 1,
    force3D: 1, autoRound: 1, onToggle: 1, ticker: 1, parent: 1, sortChildren: 1, clearProps: 1,
    modifiers: 1,
  };
  // Properties GSAP folds into one transform string.
  var TRANSFORM = {
    x: 1, y: 1, z: 1, xPercent: 1, yPercent: 1, rotation: 1, rotate: 1, rotationX: 1, rotateX: 1,
    rotationY: 1, rotateY: 1, rotationZ: 1, rotateZ: 1, scale: 1, scaleX: 1, scaleY: 1, skewX: 1,
    skewY: 1, transformPerspective: 1,
  };
  var ALIAS = { rotate: "rotation", rotationZ: "rotation", rotateZ: "rotation", rotateX: "rotationX", rotateY: "rotationY" };

  var warned = {};
  function warn(msg) {
    if (warned[msg]) return;
    warned[msg] = 1;
    send({ type: "warn", message: msg });
  }
  function send(msg) {
    if (typeof g.__blitz_send_message === "function") g.__blitz_send_message(JSON.stringify(msg));
  }

  // ---- target registry -------------------------------------------------------------------
  var nextRef = 1;
  var objects = {}; // ref -> plain JS object target
  function isElement(t) {
    return t && typeof t === "object" && typeof t.nodeType === "number" && t.nodeType === 1;
  }
  // ref -> the element itself: a target is the object (GSAP), whatever copies of its markup exist
  var refEls = {};
  function elementRef(el) {
    var r = el.getAttribute("data-hfb");
    if (!r) {
      r = String(nextRef++);
      el.setAttribute("data-hfb", r);
    }
    refEls[r] = el;
    return { el: +r };
  }
  function refElement(r) {
    return refEls[r] || document.querySelector('[data-hfb="' + r + '"]');
  }
  // data-hfb is the renderer's own tag: a copy made with cloneNode isn't a tween target, so it doesn't carry one
  // (the composition format's motion-blur snippet clones its targets; the copies' tags made the word's ref find a copy)
  (function () {
    var proto = g.Node && g.Node.prototype;
    if (!proto || typeof proto.cloneNode !== "function") return;
    var clone = proto.cloneNode;
    proto.cloneNode = function (deep) {
      var c = clone.call(this, deep);
      if (c && c.nodeType === 1) {
        if (c.hasAttribute("data-hfb")) c.removeAttribute("data-hfb");
        if (deep) c.querySelectorAll("[data-hfb]").forEach(function (n) { n.removeAttribute("data-hfb"); });
      }
      return c;
    };
  })();
  // An image source for canvas/WebGL replay: the element, plus an <img>'s src so Rust can decode images
  // Blitz never loaded (detached `new Image()` / createElement images aren't in the document).
  var canvasSources = {};
  function imageSourceRef(el) {
    var r = { ref: elementRef(el).el };
    if (String(el.tagName).toLowerCase() === "img" && el.getAttribute("src")) r.src = el.getAttribute("src");
    // canvases a frame reads from (drawImage, createPattern): their pending calls go along with getImageData
    if (String(el.tagName).toLowerCase() === "canvas") canvasSources[r.ref] = el;
    return r;
  }
  function objectRef(obj) {
    if (!Object.prototype.hasOwnProperty.call(obj, "__hfbRef")) {
      var r = nextRef++;
      Object.defineProperty(obj, "__hfbRef", { value: r, enumerable: false });
      objects[r] = obj;
    }
    return { obj: obj.__hfbRef };
  }
  // Sub-composition scripts run with a scope (their composition root): selector strings
  // resolve inside it first, like the composition format's scoped composition scripts.
  var currentScope = null;
  function withScope(scope, fn) {
    var prev = currentScope;
    currentScope = scope || prev;
    try { return fn(); } finally { currentScope = prev; }
  }
  function scopedAll(root, sel) {
    var out = [];
    try { if (root.matches && root.matches(sel)) out.push(root); } catch (e) {}
    var list;
    try { list = root.querySelectorAll(sel); } catch (e) { list = []; }
    for (var i = 0; i < list.length; i++) out.push(list[i]);
    return out;
  }
  function toArray(targets, scope) {
    if (targets == null) return [];
    if (typeof targets === "string") {
      var root = scope || currentScope;
      if (root && root !== document) {
        var inScope = scopedAll(root, targets);
        if (inScope.length) return inScope;
      }
      var list = document.querySelectorAll(targets);
      return Array.prototype.slice.call(list);
    }
    if (isElement(targets)) return [targets];
    if (Array.isArray(targets)) {
      var out = [];
      for (var i = 0; i < targets.length; i++) out = out.concat(toArray(targets[i], scope));
      return out;
    }
    if (typeof targets === "object" && typeof targets.length === "number" && !targets.nodeType) {
      return Array.prototype.slice.call(targets).filter(function (t) { return t != null; });
    }
    return [targets];
  }

  // ---- eases (JS side: only needed for stagger distribution and parseEase) -------------
  function makeEase(name) {
    if (typeof name === "function") return name;
    var s = String(name || "power1.out").trim();
    if (customEaseFns[s]) return customEaseFns[s];
    if (customEases[s]) return customEases[s];
    var m = s.match(/^([a-zA-Z0-9]+)(?:\.(in|out|inOut))?(?:\(([^)]*)\))?$/);
    var base = m ? m[1] : "power1";
    var dir = m && m[2] ? m[2] : "out";
    var arg = m && m[3] ? parseFloat(m[3]) : NaN;
    var p = { linear: 0, none: 0, power0: 0, power1: 1, quad: 1, power2: 2, cubic: 2, power3: 3, quart: 3, power4: 4, quint: 4, strong: 4 }[base];
    var fin;
    if (p !== undefined) fin = function (t) { return Math.pow(t, p + 1); };
    else if (base === "sine") fin = function (t) { return 1 - Math.cos((t * Math.PI) / 2); };
    else if (base === "expo") fin = function (t) { return t ? Math.pow(2, 10 * (t - 1)) : 0; };
    else if (base === "circ") fin = function (t) { return 1 - Math.sqrt(1 - t * t); };
    else if (base === "back") { var o = isNaN(arg) ? 1.70158 : arg; fin = function (t) { return t * t * ((o + 1) * t - o); }; }
    else if (base === "steps") { var n = isNaN(arg) ? 1 : arg; return function (t) { return Math.min(1, Math.floor(t * n) / n); }; }
    else fin = function (t) { return t * t; };
    if (dir === "in") return fin;
    if (dir === "out") return function (t) { return 1 - fin(1 - t); };
    return function (t) { return t < 0.5 ? fin(t * 2) / 2 : 1 - fin((1 - t) * 2) / 2; };
  }
  var customEases = {};

  // ---- deterministic random (GSAP's random() strings; the composition format forbids Math.random) --
  var seed = 0x2f6b9a1;
  function rand() {
    seed = (seed + 0x6d2b79f5) | 0;
    var t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }
  function randomValue(min, max, snap) {
    if (Array.isArray(min)) return min[Math.floor(rand() * min.length)];
    var v = min + rand() * (max - min);
    return snap ? Math.round(v / snap) * snap : v;
  }
  function resolveRandomString(v) {
    if (typeof v !== "string" || v.indexOf("random(") < 0) return v;
    return v.replace(/random\(([^)]*)\)/g, function (_, args) {
      var a = args.trim();
      if (a.charAt(0) === "[") return String(randomValue(JSON.parse(a)));
      var p = a.split(",").map(parseFloat);
      return String(randomValue(p[0], p[1], p[2]));
    });
  }

  // ---- stagger ---------------------------------------------------------------------------
  function staggerDelays(stagger, targets) {
    var n = targets.length;
    var delays = new Array(n);
    if (typeof stagger === "function") {
      for (var i = 0; i < n; i++) delays[i] = +stagger(i, targets[i], targets) || 0;
      return delays;
    }
    var cfg = typeof stagger === "object" ? stagger : { each: +stagger };
    var from = cfg.from == null ? 0 : cfg.from;
    if (cfg.grid) warn("stagger.grid is treated as a 1-D stagger");
    var origin;
    if (from === "start") origin = 0;
    else if (from === "end") origin = n - 1;
    else if (from === "center" || from === "edges") origin = (n - 1) / 2;
    else if (from === "random") origin = -1;
    else origin = +from || 0;
    var dist = [];
    var maxDist = 0;
    for (var j = 0; j < n; j++) {
      var d = origin < 0 ? j : Math.abs(j - origin);
      dist.push(d);
      if (d > maxDist) maxDist = d;
    }
    if (from === "edges") for (var k = 0; k < n; k++) dist[k] = maxDist - dist[k];
    if (from === "random") {
      // Fisher-Yates over the deterministic generator
      for (var r = n - 1; r > 0; r--) { var q = Math.floor(rand() * (r + 1)); var tmp = dist[r]; dist[r] = dist[q]; dist[q] = tmp; }
    }
    var each = cfg.amount != null ? (maxDist ? cfg.amount / maxDist : 0) : +cfg.each || 0;
    var ease = cfg.ease ? makeEase(cfg.ease) : null;
    for (var m = 0; m < n; m++) {
      var x = maxDist ? dist[m] / maxDist : 0;
      delays[m] = ease ? ease(x) * maxDist * each : dist[m] * each;
    }
    return delays;
  }

  // ---- animations ------------------------------------------------------------------------
  var nextId = 1;
  var creation = 0;
  var globalDefaults = { duration: 0.5, ease: "power1.out" };
  var callbacks = {}; // id -> { fn, params, scope }
  var animsById = {};
  function registerCallback(fn, params, scope) {
    if (typeof fn !== "function") return null;
    var id = nextId++;
    callbacks[id] = { fn: fn, params: params || [], scope: scope };
    return id;
  }

  function Animation(vars) {
    this.vars = vars || {};
    this._id = nextId++;
    this._start = 0;
    this._delay = +this.vars.delay || 0;
    this._repeat = this.vars.repeat | 0;
    this._yoyo = !!this.vars.yoyo;
    this._rDelay = +this.vars.repeatDelay || 0;
    this._ts = this.vars.timeScale != null ? +this.vars.timeScale : 1;
    this.parent = null;
    this._paused = !!this.vars.paused;
    this._cb = {};
    // the animation's time as of the current frame (set by Rust with each callback, and by hf-seek for
    // registered timelines)
    this._uvT = this._uvTT = this._uvP = 0;
    animsById[this._id] = this;
    var self = this;
    ["onStart", "onUpdate", "onComplete", "onRepeat"].forEach(function (k) {
      var id = registerCallback(self.vars[k], self.vars[k + "Params"], self.vars.callbackScope);
      if (id) self._cb[k] = id;
    });
  }
  Animation.prototype.duration = function (v) {
    if (v === undefined) return this._dur();
    var d = this._dur();
    if (d > 0 && v > 0) this._ts = d / v;
    return this;
  };
  Animation.prototype.totalDuration = function () {
    var d = this._dur() / (this._ts || 1);
    if (this._repeat < 0) return 1e8;
    return d * (this._repeat + 1) + this._rDelay * this._repeat;
  };
  Animation.prototype.endTime = function () { return this._start + this.totalDuration(); };
  Animation.prototype.startTime = function (v) {
    if (v === undefined) return this._start;
    this._start = +v;
    return this;
  };
  Animation.prototype.delay = function (v) {
    if (v === undefined) return this._delay;
    this._start += v - this._delay;
    this._delay = v;
    return this;
  };
  Animation.prototype.timeScale = function (v) {
    if (v === undefined) return this._ts;
    this._ts = +v || 1;
    return this;
  };
  Animation.prototype.repeat = function (v) { if (v === undefined) return this._repeat; this._repeat = v | 0; return this; };
  Animation.prototype.yoyo = function (v) { if (v === undefined) return this._yoyo; this._yoyo = !!v; return this; };
  Animation.prototype.repeatDelay = function (v) { if (v === undefined) return this._rDelay; this._rDelay = +v || 0; return this; };
  Animation.prototype.eventCallback = function (type, fn, params) {
    if (fn === undefined) return this.vars[type];
    var id = registerCallback(fn, params, this.vars.callbackScope);
    if (id) this._cb[type] = id; else delete this._cb[type];
    return this;
  };
  // Playback controls are owned by the renderer; authored calls are accepted and ignored,
  // except that pause()/play() decide whether an unregistered timeline runs or stays frozen.
  Animation.prototype.pause = function () { this._paused = true; return this; };
  Animation.prototype.play = Animation.prototype.resume = Animation.prototype.restart = function () { this._paused = false; return this; };
  Animation.prototype.paused = function (v) {
    if (v === undefined) return this._paused;
    this._paused = !!v;
    return this;
  };
  ["reverse", "reverse", "restart", "seek", "invalidate", "kill", "revert", "totalTime", "totalProgress", "progress", "time", "iteration", "reversed", "isActive", "then", "render", "globalTime", "scrollTrigger"].forEach(function (k) {
    Animation.prototype[k] = function (v) {
      if (k === "time" || k === "totalTime" || k === "progress" || k === "totalProgress") {
        if (v !== undefined) {
          scriptSeek(this, k === "progress" || k === "totalProgress" ? (+v || 0) * (this._dur ? this._dur() : 0) : +v);
          return this;
        }
        return k === "time" ? this._uvT : k === "totalTime" ? this._uvTT : this._uvP;
      }
      if (k === "seek") {
        if (v !== undefined) scriptSeek(this, typeof v === "string" ? (this.labels || {})[v] : +v);
        return this;
      }
      if (k === "isActive" || k === "reversed") return false;
      if (k === "iteration") return 1;
      if (k === "then") return Promise.resolve(this);
      return this;
    };
  });

  // A script seeking a timeline while the page runs (tl.time(x), seek, progress): the renderer samples the
  // timeline's tracks there and the values go on the elements, so computed styles read what GSAP would render
  // (the composition format's motion-blur snippet reads its shutter window this way). During setup the renderer has no
  // timelines yet and the call is ignored, as it is for tweens.
  function scriptSeek(anim, local) {
    if (!live || !(anim instanceof Timeline) || !(local >= 0)) return;
    var out;
    try { out = JSON.parse(__blitz_native("uvSample", JSON.stringify({ id: anim._id, t: local }))); } catch (e) { return; }
    for (var i = 0; i < out.length; i++) {
      var el = refElement(out[i][0]);
      if (!el) continue;
      var d = out[i][1];
      for (var p in d) {
        if (d[p] === null) el.style.removeProperty(p);
        else el.style.setProperty(p, d[p]);
      }
    }
    var span = anim._dur ? anim._dur() : 0;
    anim._uvT = anim._uvTT = local;
    anim._uvP = span > 0 ? Math.min(1, local / span) : 1;
  }

  function Tween(targets, vars, mode, fromVars) {
    Animation.call(this, vars);
    this._mode = mode;
    this._targets = targets;
    this._fromVars = fromVars || null;
    this._d = vars.duration != null ? +vars.duration : mode === "set" ? 0 : globalDefaults.duration;
    this._creation = creation++;
    this._node = null;
    this._scope = currentScope;
    this._build();
  }
  Tween.prototype = Object.create(Animation.prototype);
  Tween.prototype.constructor = Tween;
  Tween.prototype._dur = function () { return this._staggerSpan + this._d; };
  Tween.prototype.targets = function () { return this._targets.slice(); };

  function camelToKebab(p) {
    if (p.indexOf("--") === 0) return p;
    return p.replace(/[A-Z]/g, function (c) { return "-" + c.toLowerCase(); });
  }
  var DOM_PROPS = { innerText: 1, textContent: 1, innerHTML: 1, value: 1, scrollTop: 1, scrollLeft: 1 };
  function baseValue(target, prop, cs) {
    if (isElement(target)) {
      if (prop === "attr") return null;
      if (prop === "--uv-draw") return drawSVGBase(target);
      if (prop.indexOf("--uv-motion-") === 0 || prop.indexOf("--uv-morph-") === 0 || prop.indexOf("--uv-text-") === 0) return "0";
      if (DOM_PROPS[prop]) return target[prop] == null ? "" : String(target[prop]);
      if (TRANSFORM[prop] || ALIAS[prop]) return null; // transform base comes from the element's computed matrix
      if (prop === "autoAlpha") prop = "opacity";
      var v = cs ? cs.getPropertyValue(camelToKebab(prop)) : "";
      if ((v === "" || v == null) && target.style) v = target.style[prop];
      return v == null ? "" : String(v);
    }
    var raw = target[prop];
    return typeof raw === "function" ? raw.call(target) : raw;
  }
  function resolvePerTarget(v, i, t, targets) {
    if (typeof v === "function") v = v(i, t, targets);
    return cqToPx(t, resolveRandomString(v));
  }
  function propsFor(vars, i, t, targets) {
    var props = {};
    for (var k in vars) {
      if (RESERVED[k] === 1) continue;
      var v = vars[k];
      if (k === "attr" || k === "css") {
        var sub = {};
        for (var a in v) sub[a] = resolvePerTarget(v[a], i, t, targets);
        if (k === "css") { for (var c in sub) props[c] = sub[c]; continue; }
        props.attr = sub;
        continue;
      }
      if (k === "drawSVG") {
        // DrawSVGPlugin: tween "dash offset length" (Rust formats the stroke-dash* values)
        if (isElement(t)) props["--uv-draw"] = drawSVGValue(t, resolvePerTarget(v, i, t, targets));
        continue;
      }
      if (k === "motionPath") {
        // MotionPathPlugin: the path spec rides on the element; Rust evaluates the path per frame
        // from a progress tween "--uv-motion-N" (0 -> 1 with the tween's ease)
        var mprop = isElement(t) && motionSpec(t, resolvePerTarget(v, i, t, targets));
        if (mprop) props[mprop] = 1;
        continue;
      }
      if (k === "morphSVG") {
        // MorphSVGPlugin (linear): Rust equalizes the shapes once and writes `d` per frame from
        // a progress tween "--uv-morph-N"
        var mo = isElement(t) && morphSpec(t, resolvePerTarget(v, i, t, targets));
        if (mo) props[mo] = 1;
        continue;
      }
      if (k === "text") {
        // TextPlugin: Rust writes the plugin's string for the progress tween "--uv-text-N"
        var tx = isElement(t) && textSpec(t, resolvePerTarget(v, i, t, targets));
        if (tx) props[tx] = 1;
        continue;
      }
      if (k === "scrambleText" || k === "physics2D" || k === "inertia") {
        warn("GSAP plugin property '" + k + "' is not supported by the blitz renderer");
        continue;
      }
      var rv = typeof v === "function" ? v(i, t, targets) : v;
      rv = resolveRandomString(rv);
      // container units convert again when the tween is handed over (GSAP converts at first render)
      if (typeof rv === "string" && CQ_ANY.test(rv) && isElement(t)) {
        if (!props.__uvCq) Object.defineProperty(props, "__uvCq", { value: {}, enumerable: false });
        props.__uvCq[ALIAS[k] || k] = rv;
      }
      props[ALIAS[k] || k] = cqToPx(t, rv);
    }
    return props;
  }
  var CQ_ANY = /\d(cqw|cqi|cqh|cqb|cqmin|cqmax)\b/i;
  function jsonValue(v) {
    if (v == null) return null;
    if (typeof v === "number" || typeof v === "boolean") return v;
    return String(v);
  }
  function encodeProps(props) {
    var out = {};
    for (var k in props) {
      if (k === "attr") {
        var a = {};
        for (var n in props.attr) a[n] = jsonValue(props.attr[n]);
        out.attr = a;
      } else out[k] = jsonValue(props[k]);
    }
    return out;
  }

  Tween.prototype._build = function () {
    var targets = toArray(this._targets, this._scope);
    this._targets = targets;
    var vars = this.vars;
    var stagger = vars.stagger;
    var delays = stagger != null && targets.length > 1 ? staggerDelays(stagger, targets) : targets.map(function () { return 0; });
    var span = 0;
    for (var i = 0; i < delays.length; i++) if (delays[i] > span) span = delays[i];
    this._staggerSpan = span;
    var ease = vars.ease != null ? vars.ease : null;
    if (typeof ease === "function" && ease.__uvEase) ease = ease.__uvEase;
    if (typeof ease === "function") {
      warn("function eases are approximated by sampling");
      ease = { samples: sampleEase(ease) };
    }
    var keyframes = vars.keyframes;
    var items = [];
    // TextPlugin inverts from() tweens (textSpec reads this while the props are built)
    var prevFromTween = buildingFromTween;
    buildingFromTween = this._mode === "from";
    for (var j = 0; j < targets.length; j++) {
      var t = targets[j];
      var ref = isElement(t) ? elementRef(t) : typeof t === "object" ? objectRef(t) : null;
      if (!ref) continue;
      var cs = isElement(t) && typeof g.getComputedStyle === "function" ? g.getComputedStyle(t) : null;
      var to = propsFor(vars, j, t, targets);
      var from = this._fromVars ? propsFor(this._fromVars, j, t, targets) : vars.startAt ? propsFor(vars.startAt, j, t, targets) : null;
      var base = {};
      var collect = function (obj) { for (var k in obj) if (k !== "attr" && !(k in base)) base[k] = jsonValue(baseValue(t, k, cs)); };
      collect(to);
      if (from) collect(from);
      var attrBase = {};
      var attrs = Object.assign({}, from && from.attr, to.attr);
      for (var an in attrs) attrBase[an] = isElement(t) ? t.getAttribute(an) : null;
      var inline = {};
      if (isElement(t)) {
        for (var ip in base) {
          if (TRANSFORM[ip] || ALIAS[ip]) continue;
          if (DOM_PROPS[ip]) { inline[ip] = String(t[ip] == null ? "" : t[ip]); continue; }
          var cssName = camelToKebab(ip === "autoAlpha" ? "opacity" : ip);
          inline[ip] = t.style && t.style.getPropertyValue ? String(t.style.getPropertyValue(cssName) || "") : "";
        }
      }
      var item = {
        target: ref,
        offset: delays[j],
        to: encodeProps(to),
        from: from ? encodeProps(from) : null,
        base: base,
        attrBase: attrBase,
        inline: inline,
      };
      if (isElement(t) && cs && needsTransform(to, from)) {
        item.transform = cs.getPropertyValue("transform") || "none";
        item.origin = cs.getPropertyValue("transform-origin") || "";
      }
      if (keyframes) item.keyframes = encodeKeyframes(keyframes, j, t, targets, this._d);
      else if (vars.modifiers && typeof vars.modifiers === "object") {
        var modKeys = modifierKeyframes(vars.modifiers, this._mode, to, from, t, cs, vars.ease != null ? vars.ease : globalDefaults.ease, this._d);
        if (modKeys) item.keyframes = modKeys;
        else warn("modifiers on a tween without numeric start and end values are ignored");
      }
      if (to.__uvCq || (from && from.__uvCq)) {
        Object.defineProperty(item, "__uvCq", { value: { el: t, to: to.__uvCq, from: from && from.__uvCq }, enumerable: false });
      }
      items.push(item);
    }
    buildingFromTween = prevFromTween;
    if (keyframes) {
      var kfTotal = keyframesDuration(keyframes, this._d);
      if (vars.duration == null) this._d = kfTotal;
    }
    var mode = this._mode;
    var immediate = vars.immediateRender != null ? !!vars.immediateRender : mode === "from" || mode === "fromTo" || !!vars.startAt;
    this._node = {
      kind: "tween",
      id: this._id,
      mode: mode,
      creation: this._creation,
      duration: this._d,
      ease: ease,
      yoyoEase: vars.yoyoEase != null ? String(vars.yoyoEase) : null,
      immediateRender: immediate,
      clearProps: vars.clearProps == null || vars.clearProps === false ? null : vars.clearProps === true ? "all" : String(vars.clearProps),
      items: items,
    };
  };
  function needsTransform(to, from) {
    for (var k in to) if (TRANSFORM[k]) return true;
    if (from) for (var f in from) if (TRANSFORM[f]) return true;
    return false;
  }
  function sampleEase(fn) {
    var out = [];
    for (var i = 0; i <= 64; i++) out.push(+fn(i / 64) || 0);
    return out;
  }
  // keyframes: [{x:10, duration:1}, ...] | {"0%": {...}, "50%": {...}} | {x: [0, 50, 100], ease}
  function encodeKeyframes(kf, i, t, targets, totalDuration) {
    var steps = [];
    if (Array.isArray(kf)) {
      kf.forEach(function (step) {
        steps.push({ duration: step.duration != null ? +step.duration : globalDefaults.duration, delay: +step.delay || 0, ease: step.ease || null, props: encodeProps(propsFor(step, i, t, targets)) });
      });
    } else {
      var pct = Object.keys(kf).filter(function (k) { return /%$/.test(k); });
      if (pct.length) {
        pct.sort(function (a, b) { return parseFloat(a) - parseFloat(b); });
        var prev = 0;
        pct.forEach(function (k) {
          var at = parseFloat(k) / 100;
          var step = kf[k];
          steps.push({ duration: (at - prev) * totalDuration, delay: 0, ease: step.ease || kf.easeEach || "none", props: encodeProps(propsFor(step, i, t, targets)) });
          prev = at;
        });
      } else {
        var props = Object.keys(kf).filter(function (k) { return Array.isArray(kf[k]); });
        var count = props.reduce(function (n, p) { return Math.max(n, kf[p].length); }, 0);
        for (var s = 1; s < count; s++) {
          var one = {};
          props.forEach(function (p) { if (s < kf[p].length) one[p] = kf[p][s]; });
          steps.push({ duration: totalDuration / (count - 1), delay: 0, ease: kf.easeEach || "none", props: encodeProps(propsFor(one, i, t, targets)) });
        }
        if (count) {
          var first = {};
          props.forEach(function (p) { first[p] = kf[p][0]; });
          steps.unshift({ duration: 0, delay: 0, ease: "none", props: encodeProps(propsFor(first, i, t, targets)) });
        }
      }
    }
    return steps;
  }
  // ModifiersPlugin: every render passes each property's value through modifiers[prop](value, target). The tween
  // becomes keyframes sampling that per target (the tween's ease folded into the raw values; linear steps
  // between samples). Needs numeric start and end values; null otherwise.
  var MODIFIER_STEPS = 120;
  function modifierKeyframes(mods, mode, to, from, t, cs, ease, dur) {
    var ends = {};
    for (var k in to) {
      if (k === "attr") return null;
      var start = from && k in from ? from[k] : baseValue(t, k, cs);
      var end = to[k];
      if (mode === "from") { start = to[k]; end = baseValue(t, k, cs); }
      var a = parseFloat(start), z = parseFloat(end);
      if (!isFinite(a) || !isFinite(z)) return null;
      ends[k] = [a, z, String(to[k]).replace(/^\s*[-+]?[0-9.]+(e[-+]?[0-9]+)?/i, "")];
    }
    var e = makeEase(ease);
    var at = function (p) {
      var out = {}, q = p <= 0 ? 0 : p >= 1 ? 1 : e(p);
      for (var k in ends) {
        var raw = ends[k][0] + (ends[k][1] - ends[k][0]) * q;
        out[k] = typeof mods[k] === "function" ? mods[k](ends[k][2] ? raw + ends[k][2] : raw, t) : raw;
      }
      return encodeProps(out);
    };
    var steps = [{ duration: 0, delay: 0, ease: "none", props: at(0) }];
    for (var i = 1; i <= MODIFIER_STEPS; i++) steps.push({ duration: dur / MODIFIER_STEPS, delay: 0, ease: "none", props: at(i / MODIFIER_STEPS) });
    return steps;
  }
  function keyframesDuration(kf, d) {
    if (Array.isArray(kf)) return kf.reduce(function (n, s) { return n + (s.duration != null ? +s.duration : globalDefaults.duration) + (+s.delay || 0); }, 0);
    return d;
  }

  Tween.prototype._serialize = function () {
    var n = this._node;
    // raw container-unit values against the layout now (after the runtime's clip layout), as GSAP's first render
    (n.items || []).forEach(function (item) {
      var c = item.__uvCq;
      if (!c) return;
      var p;
      for (p in c.to || {}) item.to[p] = jsonValue(cqToPx(c.el, c.to[p]));
      if (item.from) for (p in c.from || {}) item.from[p] = jsonValue(cqToPx(c.el, c.from[p]));
    });
    n.start = this._start;
    n.duration = this._d;
    n.ease = n.ease != null ? n.ease : this._ease || globalDefaults.ease;
    n.repeat = this._repeat;
    n.yoyo = this._yoyo;
    n.repeatDelay = this._rDelay;
    n.timeScale = this._ts;
    n.callbacks = this._cb;
    n.overwrite = this.vars.overwrite != null ? this.vars.overwrite : globalDefaults.overwrite != null ? globalDefaults.overwrite : false;
    return n;
  };

  function Timeline(vars) {
    Animation.call(this, vars || {});
    this._children = [];
    this.labels = {};
    this._recent = null;
    this._defaults = Object.assign({}, this.vars.defaults || {});
    this._scope = currentScope;
  }
  Timeline.prototype = Object.create(Animation.prototype);
  Timeline.prototype.constructor = Timeline;
  Timeline.prototype._dur = function () {
    var end = 0;
    for (var i = 0; i < this._children.length; i++) {
      var c = this._children[i];
      var e = c._start + c.totalDuration();
      if (e > end) end = e;
    }
    return end;
  };
  Timeline.prototype._parsePosition = function (position, percentAnim) {
    var labels = this.labels;
    var recent = this._recent;
    var clipped = this._dur();
    if (typeof position === "string" && (isNaN(position) || position in labels)) {
      var offset = position.charAt(0);
      var isPercent = position.substr(-1) === "%";
      var i = position.indexOf("=");
      if (offset === "<" || offset === ">") {
        if (i >= 0) position = position.replace(/=/, "");
        var rs = recent ? recent._start : 0;
        var re = recent ? recent.endTime() : 0;
        var rel = parseFloat(position.substr(1)) || 0;
        if (isPercent) rel = (rel / 100) * ((i < 0 ? recent : percentAnim) || { totalDuration: function () { return 0; } }).totalDuration();
        return (offset === "<" ? rs : re) + rel;
      }
      if (i < 0) {
        if (!(position in labels)) labels[position] = clipped;
        return labels[position];
      }
      var off = parseFloat(position.charAt(i - 1) + position.substr(i + 1));
      if (isPercent && percentAnim) off = (off / 100) * percentAnim.totalDuration();
      return i > 1 ? this._parsePosition(position.substr(0, i - 1), percentAnim) + off : clipped + off;
    }
    return position == null ? clipped : +position;
  };
  Timeline.prototype.add = function (child, position) {
    if (typeof position === "number" || position == null || typeof position === "string") {
      // fallthrough
    }
    if (Array.isArray(child)) {
      var at = this._parsePosition(position);
      for (var i = 0; i < child.length; i++) this.add(child[i], at);
      return this;
    }
    if (typeof child === "string") return this.addLabel(child, position);
    if (typeof child === "function") return this.call(child, null, position);
    if (!(child instanceof Animation)) return this;
    if (child.parent && child.parent !== this) child.parent._remove(child);
    var pos = this._parsePosition(position, child);
    child._start = pos + child._delay;
    child.parent = this;
    this._children.push(child);
    this._recent = child;
    return this;
  };
  Timeline.prototype._remove = function (child) {
    var i = this._children.indexOf(child);
    if (i >= 0) this._children.splice(i, 1);
    if (this._recent === child) this._recent = this._children[this._children.length - 1] || null;
  };
  Timeline.prototype.remove = function (child) { this._remove(child); return this; };
  Timeline.prototype.addLabel = function (label, position) {
    this.labels[label] = this._parsePosition(position);
    return this;
  };
  Timeline.prototype.removeLabel = function (label) { delete this.labels[label]; return this; };
  Timeline.prototype.addPause = function () { return this; };
  Timeline.prototype.removePause = function () { return this; };
  Timeline.prototype.getChildren = function () { return this._children.slice(); };
  Timeline.prototype.recent = function () { return this._recent; };
  Timeline.prototype.clear = function () { this._children = []; this.labels = {}; this._recent = null; return this; };
  Timeline.prototype.nextLabel = function () { return undefined; };
  Timeline.prototype.previousLabel = function () { return undefined; };
  Timeline.prototype.currentLabel = function () { return undefined; };
  Timeline.prototype.tweenTo = function () { return new Timeline({}); };
  Timeline.prototype.tweenFromTo = function () { return new Timeline({}); };
  Timeline.prototype.getTweensOf = function () { return []; };
  Timeline.prototype.killTweensOf = function () { return this; };
  Timeline.prototype._withDefaults = function (vars) {
    var out = {};
    var d = this._inheritedDefaults();
    for (var k in d) out[k] = d[k];
    for (var v in vars) out[v] = vars[v];
    return out;
  };
  Timeline.prototype._inheritedDefaults = function () {
    var chain = [];
    for (var tl = this; tl; tl = tl.parent) if (tl._defaults) chain.unshift(tl._defaults);
    return Object.assign.apply(null, [{}].concat(chain));
  };
  Timeline.prototype.to = function (targets, vars, position) {
    var self = this;
    return withScope(this._scope, function () { return self.add(new Tween(targets, self._withDefaults(vars || {}), "to"), position); });
  };
  Timeline.prototype.from = function (targets, vars, position) {
    var self = this;
    return withScope(this._scope, function () { return self.add(new Tween(targets, self._withDefaults(vars || {}), "from"), position); });
  };
  Timeline.prototype.fromTo = function (targets, fromVars, toVars, position) {
    var self = this;
    return withScope(this._scope, function () { return self.add(new Tween(targets, self._withDefaults(toVars || {}), "fromTo", fromVars || {}), position); });
  };
  Timeline.prototype.set = function (targets, vars, position) {
    var v = Object.assign({}, vars || {}, { duration: 0 });
    var self = this;
    return withScope(this._scope, function () { return self.add(new Tween(targets, v, "set"), position); });
  };
  Timeline.prototype.call = function (fn, params, position) {
    var c = new CallMarker(fn, params);
    return this.add(c, position);
  };
  Timeline.prototype._serialize = function () {
    return {
      kind: "timeline",
      id: this._id,
      start: this._start,
      duration: this._dur(),
      repeat: this._repeat,
      yoyo: this._yoyo,
      repeatDelay: this._rDelay,
      timeScale: this._ts,
      paused: !!this._paused,
      callbacks: this._cb,
      children: this._children.map(function (c) { return c._serialize(); }),
    };
  };

  function CallMarker(fn, params) {
    Animation.call(this, {});
    this._call = registerCallback(fn, params);
  }
  CallMarker.prototype = Object.create(Animation.prototype);
  CallMarker.prototype._dur = function () { return 0; };
  CallMarker.prototype._serialize = function () {
    return { kind: "call", id: this._id, start: this._start, callback: this._call };
  };

  // Tweens/timelines created outside a timeline land on the global timeline at time 0
  // (the renderer's clock starts at the composition's first frame).
  var globalTimeline = new Timeline({});
  globalTimeline._isGlobal = true;
  function attachToGlobal(anim) {
    var pos = anim._delay;
    anim._start = pos;
    anim.parent = globalTimeline;
    globalTimeline._children.push(anim);
    globalTimeline._recent = anim;
    return anim;
  }
  function ensureTween(t, vars) {
    var d = Object.assign({}, globalDefaults);
    delete d.duration;
    t._ease = vars.ease != null ? vars.ease : d.ease;
    return t;
  }

  // ---- live mode: GSAP calls made from callbacks while the renderer plays -----------------
  // After setup the authored tree is frozen in Rust; calls made later (tl.call / onUpdate
  // handlers that create elements and gsap.set them) are applied to the DOM directly.
  var live = false;
  var UNITLESS = { opacity: 1, zIndex: 1, fontWeight: 1, lineHeight: 1, flexGrow: 1, flexShrink: 1, order: 1, scale: 1, scaleX: 1, scaleY: 1, fillOpacity: 1, strokeOpacity: 1 };
  function liveTransform(el, vars) {
    var st = el.__hfbLive || (el.__hfbLive = { x: 0, y: 0, z: 0, xPercent: 0, yPercent: 0, rotation: 0, rotationX: 0, rotationY: 0, scaleX: 1, scaleY: 1, skewX: 0, skewY: 0, transformPerspective: 0 });
    for (var k in vars) {
      var p = ALIAS[k] || k;
      if (p === "scale") { st.scaleX = st.scaleY = parseFloat(vars[k]); continue; }
      if (p in st) st[p] = vars[k];
    }
    var u = function (v, d) { return typeof v === "number" ? v + d : String(v); };
    // GSAP's _renderCSSTransforms order
    var out = "";
    if (parseFloat(st.transformPerspective)) out += "perspective(" + u(st.transformPerspective, "px") + ") ";
    if (parseFloat(st.xPercent) || parseFloat(st.yPercent)) out += "translate(" + parseFloat(st.xPercent) + "%, " + parseFloat(st.yPercent) + "%) ";
    if (parseFloat(st.z)) out += "translate3d(" + u(st.x, "px") + ", " + u(st.y, "px") + ", " + u(st.z, "px") + ") ";
    else if (parseFloat(st.x) || parseFloat(st.y)) out += "translate(" + u(st.x, "px") + ", " + u(st.y, "px") + ") ";
    if (parseFloat(st.rotation)) out += "rotate(" + u(st.rotation, "deg") + ") ";
    if (parseFloat(st.rotationY)) out += "rotateY(" + u(st.rotationY, "deg") + ") ";
    if (parseFloat(st.rotationX)) out += "rotateX(" + u(st.rotationX, "deg") + ") ";
    if (parseFloat(st.skewX) || parseFloat(st.skewY)) out += "skew(" + u(st.skewX, "deg") + ", " + u(st.skewY, "deg") + ") ";
    if (st.scaleX != 1 || st.scaleY != 1) out += "scale(" + st.scaleX + ", " + st.scaleY + ") ";
    el.style.transform = out.trim() || "none";
  }
  function applyNow(targets, vars) {
    toArray(targets).forEach(function (el, i, all) {
      var resolved = {};
      for (var k in vars) if (RESERVED[k] !== 1) resolved[k] = resolvePerTarget(vars[k], i, el, all);
      if (!isElement(el)) { for (var o in resolved) el[o] = resolved[o]; return; }
      var tf = {};
      for (var p in resolved) {
        var v = resolved[p];
        if (TRANSFORM[p] || ALIAS[p] || p === "scale") { tf[p] = v; continue; }
        if (p === "attr") { for (var a in v) el.setAttribute(a, v[a]); continue; }
        if (DOM_PROPS[p]) { el[p] = v; continue; }
        if (p === "autoAlpha") {
          el.style.setProperty("opacity", String(v));
          el.style.setProperty("visibility", parseFloat(v) === 0 ? "hidden" : "inherit");
          continue;
        }
        el.style.setProperty(camelToKebab(p), typeof v === "number" && !UNITLESS[p] ? v + "px" : String(v));
      }
      if (Object.keys(tf).length) liveTransform(el, tf);
    });
  }

  var gsap = {
    version: "3.13.0-raea-blitz",
    timeline: function (vars) {
      var tl = new Timeline(vars || {});
      return attachToGlobal(tl);
    },
    to: function (targets, vars) {
      if (live) { applyNow(targets, vars || {}); warn("gsap.to() called during playback is applied instantly"); return new Timeline({}); }
      return attachToGlobal(ensureTween(new Tween(targets, Object.assign({}, vars || {}), "to"), vars || {}));
    },
    from: function (targets, vars) {
      if (live) return new Timeline({});
      return attachToGlobal(ensureTween(new Tween(targets, Object.assign({}, vars || {}), "from"), vars || {}));
    },
    fromTo: function (targets, fromVars, toVars) {
      if (live) { applyNow(targets, toVars || {}); warn("gsap.fromTo() called during playback is applied instantly"); return new Timeline({}); }
      return attachToGlobal(ensureTween(new Tween(targets, Object.assign({}, toVars || {}), "fromTo", fromVars || {}), toVars || {}));
    },
    set: function (targets, vars) {
      if (live) { applyNow(targets, vars || {}); return new Timeline({}); }
      var t = new Tween(targets, Object.assign({}, vars || {}, { duration: 0 }), "set");
      t._node.immediateRender = true;
      t._initial = true;
      return attachToGlobal(t);
    },
    delayedCall: function (delay, fn, params) {
      var c = new CallMarker(fn, params);
      c._delay = +delay || 0;
      return attachToGlobal(c);
    },
    killTweensOf: function () {},
    getTweensOf: function () { return []; },
    isTweening: function () { return false; },
    getById: function () { return null; },
    exportRoot: function () { return new Timeline({}); },
    registerPlugin: function () {},
    registerEffect: function (effect) {
      gsap.effects[effect.name] = function (targets, config) {
        return effect.effect(toArray(targets), Object.assign({}, effect.defaults || {}, config || {}));
      };
      if (effect.extendTimeline) {
        Timeline.prototype[effect.name] = function (targets, config, position) {
          return this.add(gsap.effects[effect.name](targets, config), position);
        };
      }
    },
    effects: {},
    registerEase: function (name, fn) { customEases[name] = fn; },
    parseEase: function (name) { return makeEase(name); },
    defaults: function (vars) {
      if (vars) Object.assign(globalDefaults, vars);
      return globalDefaults;
    },
    config: function () { return {}; },
    context: function (fn) {
      if (typeof fn === "function") fn(function () {});
      return { revert: function () {}, kill: function () {}, add: function (f) { if (typeof f === "function") f(); }, ignore: function (f) { f && f(); } };
    },
    matchMedia: function () {
      return { add: function (q, fn) { if (typeof fn === "function") fn({ conditions: {} }); return this; }, revert: function () {}, kill: function () {} };
    },
    getProperty: function (target, prop, unit) {
      var t = toArray(target)[0];
      if (!t) return undefined;
      if (isElement(t)) {
        var v = g.getComputedStyle(t).getPropertyValue(camelToKebab(prop));
        var n = parseFloat(v);
        return unit === undefined && !isNaN(n) && String(n) !== v ? n : v;
      }
      return t[prop];
    },
    quickSetter: function (target, prop, unit) {
      return function (v) { gsap.set(target, (function () { var o = {}; o[prop] = unit ? v + unit : v; return o; })()); };
    },
    quickTo: function (target, prop) {
      return function (v) { gsap.set(target, (function () { var o = {}; o[prop] = v; return o; })()); };
    },
    ticker: { add: function () {}, remove: function () {}, lagSmoothing: function () {}, fps: function () {}, tick: function () {}, time: 0, frame: 0, deltaRatio: function () { return 1; } },
    globalTimeline: globalTimeline,
    core: { Animation: Animation, Tween: Tween, Timeline: Timeline },
    utils: {
      toArray: function (t, scope) { return toArray(t, scope); },
      selector: function (scope) {
        var root = toArray(scope)[0] || document;
        return function (sel) { return toArray(sel, root); };
      },
      clamp: function (min, max, v) {
        var f = function (x) { return Math.min(max, Math.max(min, x)); };
        return v === undefined ? f : f(v);
      },
      mapRange: function (a, b, c, d, v) {
        var f = function (x) { return c + ((x - a) / (b - a)) * (d - c); };
        return v === undefined ? f : f(v);
      },
      normalize: function (a, b, v) {
        var f = function (x) { return (x - a) / (b - a); };
        return v === undefined ? f : f(v);
      },
      interpolate: function (a, b, p) {
        var f = function (x) {
          if (typeof a === "number") return a + (b - a) * x;
          var out = {};
          for (var k in a) out[k] = a[k] + (b[k] - a[k]) * x;
          return out;
        };
        return p === undefined ? f : f(p);
      },
      wrap: function (min, max, v) {
        var f = Array.isArray(min)
          ? function (i) { return min[((i % min.length) + min.length) % min.length]; }
          : function (x) { var r = max - min; return ((((x - min) % r) + r) % r) + min; };
        return Array.isArray(min) ? (max === undefined ? f : f(max)) : v === undefined ? f : f(v);
      },
      wrapYoyo: function (min, max, v) {
        var f = function (x) { var r = max - min, r2 = r * 2, y = ((((x - min) % r2) + r2) % r2); return min + (y > r ? r2 - y : y); };
        return v === undefined ? f : f(v);
      },
      snap: function (s, v) {
        var f = function (x) {
          if (Array.isArray(s)) return s.reduce(function (best, c) { return Math.abs(c - x) < Math.abs(best - x) ? c : best; });
          var inc = typeof s === "object" ? s.increment : s;
          return Math.round(x / inc) * inc;
        };
        return v === undefined ? f : f(v);
      },
      random: function (min, max, snap, returnFn) {
        if (returnFn === true || (Array.isArray(min) && max === true)) return function () { return randomValue(min, max, snap); };
        return randomValue(min, max, snap);
      },
      shuffle: function (a) {
        for (var i = a.length - 1; i > 0; i--) { var j = Math.floor(rand() * (i + 1)); var t = a[i]; a[i] = a[j]; a[j] = t; }
        return a;
      },
      distribute: function (cfg) {
        return function (i, t, targets) {
          var d = staggerDelays(Object.assign({}, cfg, { each: undefined, amount: 1 }), targets)[i];
          var base = cfg.base || 0;
          return base + d * (cfg.amount || 0);
        };
      },
      getUnit: function (v) { var m = String(v).match(/[\d.\-+]+([a-z%]*)/i); return m ? m[1] : ""; },
      unitize: function (fn, unit) { return function (v) { return fn(parseFloat(v)) + (unit || ""); }; },
      pipe: function () { var fns = arguments; return function (v) { for (var i = 0; i < fns.length; i++) v = fns[i](v); return v; }; },
      splitColor: function (c) {
        var m = String(c).match(/[\d.]+/g);
        return m ? m.slice(0, 4).map(Number) : [0, 0, 0];
      },
      checkPrefix: function (p) { return p; },
    },
  };

  // ---- container query units in tween values (stylo resolves them in CSS) -------------------------
  // Query containers carry a --uv-ct marker from the compile step (compile::mark_query_containers).
  function containerOf(el) {
    for (var a = el && el.parentNode; a && a.nodeType === 1; a = a.parentNode) {
      var ct = String(getComputedStyle(a).getPropertyValue("--uv-ct") || "").trim();
      if (ct === "size" || ct === "inline-size") return { el: a, type: ct };
    }
    return null;
  }
  function contentBox(el) {
    var cs = getComputedStyle(el), n = function (v) { return parseFloat(v) || 0; };
    return {
      w: Math.max(0, el.clientWidth - n(cs.paddingLeft) - n(cs.paddingRight)),
      h: Math.max(0, el.clientHeight - n(cs.paddingTop) - n(cs.paddingBottom)),
    };
  }
  // a tween value in container units ("4cqh", "-0.5cqmin") as px for this target, as GSAP converts it
  var CQ_VALUE = /(-?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?)(cqw|cqi|cqh|cqb|cqmin|cqmax)\b/gi;
  function cqToPx(target, v) {
    if (typeof v !== "string" || v.indexOf("cq") < 0 || !isElement(target)) return v;
    var c = containerOf(target), w, h;
    if (c) {
      var box = contentBox(c.el);
      w = box.w;
      if (c.type === "size") h = box.h;
      else { var up = containerOf(c.el); h = up && up.type === "size" ? contentBox(up.el).h : (g.innerHeight || 0); }
    } else {
      w = g.innerWidth || 0;
      h = g.innerHeight || 0;
    }
    return v.replace(CQ_VALUE, function (_, n, u) {
      u = u.toLowerCase();
      var f = u === "cqw" || u === "cqi" ? w : u === "cqh" || u === "cqb" ? h : u === "cqmin" ? Math.min(w, h) : Math.max(w, h);
      return Math.round(+n * f) / 100 + "px";
    });
  }

  // ---- the composition format's applyClipLayout (runtime init, after the composition's scripts) -----------------
  // The root composition is a positioned, clipped box of its data-width/height; each timed child
  // (data-start) is an absolutely positioned clip that fills the root when it has no size of its own.
  g.__uvClipLayout = function () {
    var root = document.querySelector('[data-composition-id][data-root="true"]') || document.querySelector("[data-composition-id]");
    if (!root || !root.style) return;
    if (!root.style.position) root.style.position = "relative";
    root.style.overflow = "hidden";
    var px = function (v) { var n = parseFloat(v); return isFinite(n) && n > 0 ? n + "px" : null; };
    var rw = px(root.getAttribute("data-width")), rh = px(root.getAttribute("data-height"));
    if (rw) root.style.width = rw;
    if (rh) root.style.height = rh;
    var clips = Array.prototype.filter.call(root.children || [], function (el) {
      var tag = String(el.tagName).toLowerCase();
      return ["script", "style", "link", "meta"].indexOf(tag) < 0 && el.hasAttribute("data-start") && !el.hasAttribute("data-hf-autostamped");
    });
    // display:none clips are measured shown, then hidden again
    var hidden = clips.filter(function (el) { return el.style.getPropertyValue("display") === "none"; })
      .map(function (el) { return { el: el, pr: el.style.getPropertyPriority ? el.style.getPropertyPriority("display") : "" }; });
    hidden.forEach(function (h) { h.el.style.removeProperty("display"); });
    clips.forEach(function (el) {
      var tag = String(el.tagName).toLowerCase(), cs = getComputedStyle(el);
      if (cs.position !== "absolute" && cs.position !== "fixed") el.style.position = "absolute";
      if (tag === "audio") return;
      cs = getComputedStyle(el); // Chrome's computed style is live: sizes after the switch to absolute
      var meaningful = function (v) { return v !== "0px" && v !== "auto"; };
      var fw = px(el.getAttribute("data-width")), fh = px(el.getAttribute("data-height"));
      if (fw) { if (!el.style.width && !meaningful(cs.width)) el.style.width = fw; }
      else if (!el.style.width && cs.width === "0px") el.style.width = "100%";
      if (fh) { if (!el.style.height && !meaningful(cs.height)) el.style.height = fh; }
      else if (!el.style.height && cs.height === "0px") el.style.height = "100%";
    });
    hidden.forEach(function (h) { h.el.style.setProperty("display", "none", h.pr); });
  };

  // ---- SplitText: GSAP's algorithm (gsap/SplitText.js 3.15) for text and nested inline elements ----
  function SplitText(target, vars) {
    vars = vars || {};
    var self = this, type = String(vars.type || "chars,words,lines");
    var splitLines = type.indexOf("lines") > -1, splitChars = type.indexOf("chars") > -1, splitWords = type.indexOf("words") > -1;
    var aria = vars.aria || "auto", tag = vars.tag || "div";
    this.vars = vars;
    this.chars = [];
    this.words = [];
    this.lines = [];
    this.masks = [];
    this.isSplit = true;
    this.elements = toArray(target);
    this._orig = [];
    function wrapper(kind, coll) {
      var cls = vars[kind + "sClass"] || "", inc = cls.indexOf("++") > -1;
      if (inc) cls = cls.replace("++", "");
      var make = function (text) {
        var el = document.createElement(tag), i = coll.length + 1;
        if (cls) el.className = cls + (inc ? " " + cls + i : "");
        if (vars.propIndex) el.style.setProperty("--" + kind, String(i));
        if (aria !== "none") el.setAttribute("aria-hidden", "true");
        if (tag !== "span") {
          el.style.position = "relative";
          el.style.display = kind === "line" ? "block" : "inline-block";
        }
        el.textContent = text;
        coll.push(el);
        return el;
      };
      return make;
    }
    function splitInto(element, wordW, charW) {
      var nodes = Array.prototype.slice.call(element.childNodes);
      for (var i = 0; i < nodes.length; i++) {
        var node = nodes[i];
        if (node.nodeType === 3) {
          var text = String(node.textContent || "").replace(/\s+/g, " ");
          var words = text.split(" ");
          var endsWithSpace = !words[words.length - 1];
          if (!words[words.length - 1]) words.pop();
          if (words.length && !words[0]) { element.insertBefore(document.createTextNode(" "), node); words.shift(); }
          for (var j = 0; j < words.length; j++) {
            var w = words[j];
            var wordEl = wordW(charW ? "" : w);
            element.insertBefore(wordEl, node);
            if (charW) Array.from(w).forEach(function (ch) { wordEl.appendChild(charW(ch)); });
            if (j < words.length - 1 || endsWithSpace) element.insertBefore(document.createTextNode(" "), node);
          }
          element.removeChild(node);
        } else if (node.nodeType === 1 && String(node.tagName).toLowerCase() !== "br") {
          splitInto(node, wordW, charW);
        }
      }
    }
    var empty = { left: 0, top: 0, width: 0, height: 0 };
    this.elements.forEach(function (el) {
      self._orig.push(el.innerHTML);
      if (aria === "auto") el.setAttribute("aria-label", String(el.textContent || "").trim());
      else if (aria === "hidden") el.setAttribute("aria-hidden", "true");
      var chars = [], words = [], lines = [];
      var charW = splitChars ? wrapper("char", chars) : null, wordW = wrapper("word", words);
      splitInto(el, wordW, charW);
      if (splitLines) {
        var lineW = wrapper("line", lines), cs = getComputedStyle(el), align = (cs && cs.textAlign) || "left";
        var nodes = Array.prototype.slice.call(el.childNodes);
        var bounds = nodes.map(function (n) { return n.nodeType === 1 ? n.getBoundingClientRect() : empty; });
        var wrapLine = function (a, b) {
          var line = lineW("");
          line.style.textAlign = align;
          el.insertBefore(line, nodes[a]);
          for (; a < b; a++) line.appendChild(nodes[a]);
        };
        var lineStart = 0, last = empty, k;
        for (k = 0; k < nodes.length; k++) {
          var n = nodes[k];
          if (n.nodeType !== 1) continue;
          if (String(n.tagName).toLowerCase() === "br") {
            wrapLine(lineStart, k + 1);
            el.removeChild(n);
            lineStart = k + 1;
            last = empty;
            continue;
          }
          var b = bounds[k];
          if (k && b.top > last.top && b.left < last.left + last.width - 1) {
            wrapLine(lineStart, k);
            lineStart = k;
          }
          last = b;
        }
        if (lineStart < nodes.length) wrapLine(lineStart, nodes.length);
      }
      if (!splitWords) {
        words.forEach(function (word) {
          var next = word.nextSibling;
          if (splitChars || !next || next.nodeType !== 3) {
            while (word.firstChild) word.parentNode.insertBefore(word.firstChild, word);
            word.parentNode.removeChild(word);
          } else {
            next.textContent = (word.textContent || "") + (next.textContent || "");
            word.parentNode.removeChild(word);
          }
        });
        words.length = 0;
      }
      self.lines.push.apply(self.lines, lines);
      self.words.push.apply(self.words, words);
      self.chars.push.apply(self.chars, chars);
    });
    var mask = vars.mask;
    if (mask && this[mask]) {
      this[mask].forEach(function (el) {
        var m = el.cloneNode(false);
        el.parentNode.insertBefore(m, el);
        m.appendChild(el);
        if (el.className) m.className = String(el.className).trim().split(/\s+/).map(function (c) { return c + "-mask"; }).join(" ");
        m.style.overflow = "clip";
        self.masks.push(m);
      });
    }
    if (typeof vars.onSplit === "function") vars.onSplit(this);
  }
  SplitText.prototype.revert = function () {
    var self = this;
    this.elements.forEach(function (el, i) { if (self._orig[i] !== undefined) el.innerHTML = self._orig[i]; });
    this.chars = []; this.words = []; this.lines = []; this.masks = [];
    this.isSplit = false;
    return this;
  };
  SplitText.prototype.split = function (vars) { this.revert(); SplitText.call(this, this.elements, vars || this.vars); return this; };
  SplitText.prototype.kill = function () {};
  SplitText.create = function (t, v) { return new SplitText(t, v); };

  // CustomEase: a single cubic stays an exact cubic-bezier for Rust; any other path (several segments, lines,
  // S/Q curves) becomes 257 samples of y(x) solved on its curves (Rust interpolates between them).
  function easePathSegments(data) {
    var toks = String(data).match(/[a-zA-Z]|-?(?:\d*\.\d+|\d+\.?)(?:e[-+]?\d+)?/g) || [];
    var segs = [], x = 0, y = 0, sx = 0, sy = 0, cmd = "M", i = 0, lastC = null;
    var num = function () { return parseFloat(toks[i++]); };
    while (i < toks.length) {
      if (/[a-zA-Z]/.test(toks[i])) cmd = toks[i++];
      var rel = cmd === cmd.toLowerCase(), C = cmd.toUpperCase(), ox = rel ? x : 0, oy = rel ? y : 0;
      if (C === "M") { x = num() + ox; y = num() + oy; sx = x; sy = y; cmd = rel ? "l" : "L"; lastC = null; }
      else if (C === "L") { var lx = num() + ox, ly = num() + oy; segs.push([x, y, x + (lx - x) / 3, y + (ly - y) / 3, x + 2 * (lx - x) / 3, y + 2 * (ly - y) / 3, lx, ly]); x = lx; y = ly; lastC = null; }
      else if (C === "H") { var hx = num() + ox; segs.push([x, y, x + (hx - x) / 3, y, x + 2 * (hx - x) / 3, y, hx, y]); x = hx; lastC = null; }
      else if (C === "V") { var vy = num() + oy; segs.push([x, y, x, y + (vy - y) / 3, x, y + 2 * (vy - y) / 3, x, vy]); y = vy; lastC = null; }
      else if (C === "C") { var c = [num() + ox, num() + oy, num() + ox, num() + oy, num() + ox, num() + oy]; segs.push([x, y].concat(c)); x = c[4]; y = c[5]; lastC = [c[2], c[3]]; }
      else if (C === "S") { var r1 = lastC ? [2 * x - lastC[0], 2 * y - lastC[1]] : [x, y]; var sc = [num() + ox, num() + oy, num() + ox, num() + oy]; segs.push([x, y, r1[0], r1[1]].concat(sc)); x = sc[2]; y = sc[3]; lastC = [sc[0], sc[1]]; }
      else if (C === "Q") { var q = [num() + ox, num() + oy, num() + ox, num() + oy]; segs.push([x, y, x + 2 / 3 * (q[0] - x), y + 2 / 3 * (q[1] - y), q[2] + 2 / 3 * (q[0] - q[2]), q[3] + 2 / 3 * (q[1] - q[3]), q[2], q[3]]); x = q[2]; y = q[3]; lastC = null; }
      else if (C === "Z") { x = sx; y = sy; lastC = null; }
      else i++;
    }
    return segs;
  }
  function customEaseFn(segs) {
    if (!segs.length) return null;
    var x0 = segs[0][0], y0 = segs[0][1], x1 = segs[segs.length - 1][6], y1 = segs[segs.length - 1][7];
    // paths drawn in another box are normalized so they run from (0,0) to (1,1)
    var nx = Math.abs(x1 - x0) > 1e-9 && (Math.abs(x0) > 1e-6 || Math.abs(x1 - 1) > 1e-6), ny = nx && Math.abs(y1 - y0) > 1e-9;
    var sxf = function (v) { return nx ? (v - x0) / (x1 - x0) : v; }, syf = function (v) { return ny ? (v - y0) / (y1 - y0) : v; };
    segs = segs.map(function (g) { return [sxf(g[0]), syf(g[1]), sxf(g[2]), syf(g[3]), sxf(g[4]), syf(g[5]), sxf(g[6]), syf(g[7])]; });
    var bez = function (a, b, c, d, t) { var u = 1 - t; return u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * d; };
    return function (p) {
      if (p <= 0) return segs[0][1];
      if (p >= 1) return segs[segs.length - 1][7];
      for (var k = 0; k < segs.length; k++) {
        var g = segs[k];
        if (p <= Math.max(g[0], g[6]) + 1e-12 || k === segs.length - 1) {
          var lo = 0, hi = 1, inc = g[6] >= g[0];
          for (var it = 0; it < 40; it++) {
            var mid = (lo + hi) / 2, xm = bez(g[0], g[2], g[4], g[6], mid);
            if ((xm < p) === inc) lo = mid; else hi = mid;
          }
          return bez(g[1], g[3], g[5], g[7], (lo + hi) / 2);
        }
      }
      return 1;
    };
  }
  var customEaseFns = {};
  function CustomEase() {}
  CustomEase.create = function (name, data) {
    var segs = Array.isArray(data) ? [[0, 0].concat(data.slice(0, 4).map(Number), [1, 1])] : easePathSegments(data);
    var fn = customEaseFns[name] = customEaseFn(segs) || makeEase("power1.out");
    var g0 = segs[0];
    if (segs.length === 1 && g0[0] === 0 && g0[1] === 0 && g0[6] === 1 && g0[7] === 1) {
      customEases[name] = "cubic-bezier(" + g0.slice(2, 6).join(",") + ")";
    } else {
      var samples = [];
      for (var i = 0; i <= 256; i++) samples.push(+fn(i / 256) || 0);
      customEases[name] = { samples: samples };
    }
    fn.__uvEase = customEases[name];
    return fn;
  };
  CustomEase.get = function (name) { return customEaseFns[name]; };

  var stub = { register: function () {}, create: function () { return { kill: function () {} }; }, refresh: function () {}, getAll: function () { return []; }, update: function () {}, defaults: function () {}, config: function () {} };

  g.gsap = gsap;
  g.TweenMax = g.TweenLite = { to: gsap.to, from: gsap.from, fromTo: gsap.fromTo, set: gsap.set, delayedCall: gsap.delayedCall };
  g.TimelineMax = g.TimelineLite = function (vars) { return gsap.timeline(vars); };
  g.SplitText = SplitText;
  g.CustomEase = CustomEase;
  g.ScrollTrigger = stub;
  g.MotionPathPlugin = g.DrawSVGPlugin = g.MorphSVGPlugin = g.TextPlugin = g.Flip = g.Observer = g.ScrollToPlugin = g.Draggable = g.CustomBounce = g.CustomWiggle = g.EasePack = g.PixiPlugin = g.Physics2DPlugin = g.InertiaPlugin = g.ScrambleTextPlugin = stub;
  if (!g.window) g.window = g;
  g.__timelines = g.__timelines || {};

  // ---- SVG geometry (Blitz's DOM has no SVGGeometryElement API) ------------------------------
  function numAttr(el, n) { return parseFloat(el.getAttribute(n)) || 0; }
  function flattenPath(d) {
    var toks = String(d || "").match(/[a-zA-Z]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?/g) || [];
    var pts = [], subStart = null, x = 0, y = 0, cmd = null, i = 0, lcx = null, lcy = null, lqx = null, lqy = null;
    var polys = [];
    var cur = null;
    function moveTo(nx, ny) { cur = [[nx, ny]]; polys.push(cur); x = nx; y = ny; subStart = [nx, ny]; }
    function lineTo(nx, ny) { if (!cur) moveTo(x, y); cur.push([nx, ny]); x = nx; y = ny; }
    function cubic(x1, y1, x2, y2, ex, ey) {
      var sx = x, sy = y;
      for (var k = 1; k <= 128; k++) {
        var t = k / 128, mt = 1 - t;
        lineTo(mt * mt * mt * sx + 3 * mt * mt * t * x1 + 3 * mt * t * t * x2 + t * t * t * ex,
               mt * mt * mt * sy + 3 * mt * mt * t * y1 + 3 * mt * t * t * y2 + t * t * t * ey);
      }
    }
    function quad(x1, y1, ex, ey) {
      var sx = x, sy = y;
      for (var k = 1; k <= 64; k++) {
        var t = k / 64, mt = 1 - t;
        lineTo(mt * mt * sx + 2 * mt * t * x1 + t * t * ex, mt * mt * sy + 2 * mt * t * y1 + t * t * ey);
      }
    }
    function arc(rx, ry, rot, large, sweep, ex, ey) {
      var sx = x, sy = y;
      if (!rx || !ry) { lineTo(ex, ey); return; }
      rx = Math.abs(rx); ry = Math.abs(ry);
      var phi = rot * Math.PI / 180, cp = Math.cos(phi), sp = Math.sin(phi);
      var dx = (sx - ex) / 2, dy = (sy - ey) / 2;
      var x1p = cp * dx + sp * dy, y1p = -sp * dx + cp * dy;
      var lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
      if (lam > 1) { rx *= Math.sqrt(lam); ry *= Math.sqrt(lam); }
      var num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
      var den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
      var co = (large === sweep ? -1 : 1) * Math.sqrt(Math.max(0, num / den));
      var cxp = co * rx * y1p / ry, cyp = -co * ry * x1p / rx;
      var cx = cp * cxp - sp * cyp + (sx + ex) / 2, cy = sp * cxp + cp * cyp + (sy + ey) / 2;
      function ang(ux, uy, vx, vy) { var a = Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy); return a; }
      var t1 = ang(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry);
      var dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
      if (!sweep && dt > 0) dt -= 2 * Math.PI; else if (sweep && dt < 0) dt += 2 * Math.PI;
      var n = Math.max(16, Math.ceil(Math.abs(dt) / (Math.PI / 128)));
      for (var k = 1; k <= n; k++) {
        var a = t1 + dt * k / n;
        lineTo(cx + rx * Math.cos(a) * cp - ry * Math.sin(a) * sp, cy + rx * Math.cos(a) * sp + ry * Math.sin(a) * cp);
      }
      x = ex; y = ey;
    }
    function n() { return parseFloat(toks[i++]); }
    while (i < toks.length) {
      if (/[a-zA-Z]/.test(toks[i])) cmd = toks[i++];
      var rel = cmd === cmd.toLowerCase(), c = cmd.toUpperCase(), ox = rel ? x : 0, oy = rel ? y : 0;
      if (c === "Z") { if (subStart) lineTo(subStart[0], subStart[1]); continue; }
      if (i >= toks.length) break;
      if (c === "M") { moveTo(n() + ox, n() + oy); cmd = rel ? "l" : "L"; }
      else if (c === "L") lineTo(n() + ox, n() + oy);
      else if (c === "H") lineTo(n() + ox, y);
      else if (c === "V") lineTo(x, n() + (rel ? y : 0));
      else if (c === "C") { var a1 = n() + ox, b1 = n() + oy, a2 = n() + ox, b2 = n() + oy, e1 = n() + ox, f1 = n() + oy; cubic(a1, b1, a2, b2, e1, f1); lcx = a2; lcy = b2; continue; }
      else if (c === "S") { var r1 = lcx != null ? 2 * x - lcx : x, r2 = lcy != null ? 2 * y - lcy : y; var c2 = n() + ox, d2 = n() + oy, e2 = n() + ox, f2 = n() + oy; cubic(r1, r2, c2, d2, e2, f2); lcx = c2; lcy = d2; continue; }
      else if (c === "Q") { var q1 = n() + ox, q2 = n() + oy, e3 = n() + ox, f3 = n() + oy; quad(q1, q2, e3, f3); lqx = q1; lqy = q2; continue; }
      else if (c === "T") { var t1x = lqx != null ? 2 * x - lqx : x, t1y = lqy != null ? 2 * y - lqy : y; var e4 = n() + ox, f4 = n() + oy; quad(t1x, t1y, e4, f4); lqx = t1x; lqy = t1y; continue; }
      else if (c === "A") { var rx = n(), ry = n(), rot = n(), lg = n(), sw = n(), e5 = n() + ox, f5 = n() + oy; arc(rx, ry, rot, !!lg, !!sw, e5, f5); }
      else { i++; }
      lcx = lcy = lqx = lqy = null;
    }
    return polys;
  }
  function shapePolys(el) {
    var tag = String(el.tagName || "").toLowerCase();
    if (tag === "path") return flattenPath(el.getAttribute("d"));
    if (tag === "line") return [[[numAttr(el, "x1"), numAttr(el, "y1")], [numAttr(el, "x2"), numAttr(el, "y2")]]];
    if (tag === "rect") {
      var x = numAttr(el, "x"), y = numAttr(el, "y"), w = numAttr(el, "width"), h = numAttr(el, "height");
      return [[[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]]];
    }
    if (tag === "circle" || tag === "ellipse") {
      var cx = numAttr(el, "cx"), cy = numAttr(el, "cy");
      var rx = tag === "circle" ? numAttr(el, "r") : numAttr(el, "rx"), ry = tag === "circle" ? rx : numAttr(el, "ry");
      var poly = [];
      // starts at 3 o'clock like browsers, clockwise
      for (var k = 0; k <= 96; k++) { var a = k / 96 * 2 * Math.PI; poly.push([cx + rx * Math.cos(a), cy + ry * Math.sin(a)]); }
      return [poly];
    }
    if (tag === "polyline" || tag === "polygon") {
      var nums = String(el.getAttribute("points") || "").trim().split(/[\s,]+/).map(parseFloat);
      var pts = [];
      for (var j = 0; j + 1 < nums.length; j += 2) pts.push([nums[j], nums[j + 1]]);
      if (tag === "polygon" && pts.length) pts.push(pts[0]);
      return [pts];
    }
    return [];
  }
  // ---- MotionPath: the authored spec, recorded for Rust (src/paths.rs evaluates it) -----------
  // A point array ([{x, y}, ...]) as a path string: a smooth cubic through the points (the curve
  // MotionPathPlugin's pointsToSegment draws: tangents from the neighbours, scaled by `curviness`), or
  // the points themselves as cubic control points with type "cubic". The element's current x/y (the
  // first point unless fromCurrent: false) is taken as 0,0 (the shim doesn't track it).
  var MOTION_X = ["x", "translateX", "left", "marginLeft", "xPercent"], MOTION_Y = ["y", "translateY", "top", "marginTop", "yPercent"];
  function motionPointsPath(pts, vars) {
    var xp = null, yp = null;
    for (var p in pts[0]) {
      if (!xp && MOTION_X.indexOf(p) > -1) xp = p;
      else if (!yp && MOTION_Y.indexOf(p) > -1) yp = p;
    }
    if (!xp || !yp) { warn("motionPath: point arrays need an x and a y value per point"); return null; }
    if (xp !== "x" || yp !== "y") warn("motionPath: point arrays move x/y here (" + xp + "/" + yp + " read as x/y)");
    var seg = [];
    pts.forEach(function (o) { seg.push(parseFloat(o[xp]) || 0, parseFloat(o[yp]) || 0); });
    if (vars.type !== "cubic") {
      if (vars.fromCurrent !== false) seg.unshift(0, 0);
      if (vars.relative) for (var i = 2; i < seg.length; i += 2) { seg[i] += seg[i - 2]; seg[i + 1] += seg[i - 1]; }
      seg = smoothThrough(seg, vars.curviness);
    }
    var d = "M" + seg[0] + "," + seg[1];
    for (var k = 2; k + 5 < seg.length; k += 6) d += " C" + seg.slice(k, k + 6).join(",");
    return d;
  }
  function smoothThrough(points, curviness) {
    var r5 = function (v) { return Math.round(v * 1e5) / 1e5 || 0; };
    points = points.slice();
    if (Math.abs(points[0] - points[2]) < 1e-4 && Math.abs(points[1] - points[3]) < 1e-4) points = points.slice(2);
    var l = points.length - 2, x = +points[0], y = +points[1], nx = +points[2], ny = +points[3];
    var seg = [x, y, x, y], dx2 = nx - x, dy2 = ny - y;
    var closed = Math.abs(points[l] - x) < 0.001 && Math.abs(points[l + 1] - y) < 0.001;
    if (!l) return [x, y, x, y, x, y, x, y];
    if (closed) { points.push(nx, ny); nx = x; ny = y; x = points[l - 2]; y = points[l - 1]; points.unshift(x, y); l += 4; }
    curviness = curviness || curviness === 0 ? +curviness : 1;
    for (var i = 2; i < l; i += 2) {
      var px = x, py = y;
      x = nx; y = ny; nx = +points[i + 2]; ny = +points[i + 3];
      if (x === nx && y === ny) continue;
      var dx1 = dx2, dy1 = dy2;
      dx2 = nx - x; dy2 = ny - y;
      var r1 = Math.sqrt(dx1 * dx1 + dy1 * dy1), r2 = Math.sqrt(dx2 * dx2 + dy2 * dy2);
      var r3 = Math.sqrt(Math.pow(dx2 / r2 + dx1 / r1, 2) + Math.pow(dy2 / r2 + dy1 / r1, 2));
      var tl = (r1 + r2) * curviness * 0.25 / r3;
      var mx1 = x - (x - px) * (r1 ? tl / r1 : 0), mx2 = x + (nx - x) * (r2 ? tl / r2 : 0);
      var mxm = x - (mx1 + ((mx2 - mx1) * (r1 * 3 / (r1 + r2) + 0.5) / 4 || 0));
      var my1 = y - (y - py) * (r1 ? tl / r1 : 0), my2 = y + (ny - y) * (r2 ? tl / r2 : 0);
      var mym = y - (my1 + ((my2 - my1) * (r1 * 3 / (r1 + r2) + 0.5) / 4 || 0));
      seg.push(r5(mx1 + mxm), r5(my1 + mym), r5(x), r5(y), r5(mx2 + mxm), r5(my2 + mym));
    }
    if (x !== nx || y !== ny || seg.length < 4) seg.push(r5(nx), r5(ny), r5(nx), r5(ny)); else seg.length -= 2;
    if (seg.length === 2) seg.push(x, y, x, y, x, y);
    else if (closed) { seg.splice(0, 6); seg.length -= 6; }
    return seg;
  }
  function motionSpec(target, vars) {
    if (!(vars && typeof vars === "object" && !vars.style) || !vars.path) vars = { path: vars };
    var path = vars.path, d = null, pathRef = null;
    if (typeof path === "string" && /(^[#\.][a-z]|[a-y][a-z])/i.test(path)) path = document.querySelector(path) || path;
    if (isElement(path)) {
      if (String(path.tagName).toLowerCase() !== "path") { warn("motionPath: only <path> elements and path strings are supported"); return null; }
      d = path.getAttribute("d");
      pathRef = elementRef(path).el;
    } else if (typeof path === "string") {
      d = path;
    } else if (Array.isArray(path) && path.length && typeof path[0] === "object") {
      d = motionPointsPath(path, vars);
      if (!d) return null;
    } else {
      warn("motionPath: this kind of path is not supported");
      return null;
    }
    var align = vars.align, alignRef = null;
    if (align && align !== "self") {
      var a = typeof align === "string" ? document.querySelector(align) : align;
      if (isElement(a) && a !== target) alignRef = elementRef(a).el;
    }
    var ao = vars.alignOrigin;
    if (alignRef != null && ao && ao[2] !== false) {
      target.style.transformOrigin = (ao[0] * 100) + "% " + (ao[1] * 100) + "%";
    }
    var specs = [];
    try { specs = JSON.parse(target.getAttribute("data-uv-motion") || "[]"); } catch (e) { specs = []; }
    specs.push({
      d: String(d || ""), pathRef: pathRef, align: align === "self" ? "self" : alignRef, alignOrigin: ao || null,
      autoRotate: vars.autoRotate === true ? 0 : typeof vars.autoRotate === "number" ? vars.autoRotate : (vars.autoRotate === 0 ? 0 : null),
      start: vars.start == null ? 0 : +vars.start, end: vars.end == null ? 1 : +vars.end, resolution: vars.resolution || 12,
      offsetX: +vars.offsetX || 0, offsetY: +vars.offsetY || 0,
    });
    target.setAttribute("data-uv-motion", JSON.stringify(specs));
    return "--uv-motion-" + (specs.length - 1);
  }

  // ---- MorphSVG: the target shape, recorded for Rust (src/paths.rs Morph) -----------------------
  // ---- TextPlugin: the plugin's splitting and setup, reimplemented (no GSAP code ships) ---------------
  var buildingFromTween = false;
  function textChars(text, delimiter, preserveSpaces, unescaped) {
    text += "";
    if (delimiter) return text.replace(/>/g, "&gt;").replace(/</g, "&lt;").split(delimiter);
    var out = [];
    for (var i = 0; i < text.length; i++) {
      var ch = text.charAt(i), code = ch.charCodeAt(0), next = text.charCodeAt(i + 1);
      if ((code >= 0xd800 && code <= 0xdbff) || (next >= 0xfe00 && next <= 0xfe0f)) {
        // keep a surrogate pair plus any variation selector / joiner sequence together
        var j = i + ((code >= 0xd800 && code <= 0xdbff) ? 2 : 1);
        while (j < text.length) {
          var c = text.charCodeAt(j);
          if ((c >= 0xfe00 && c <= 0xfe0f) || c === 0x200d) { j += 1; continue; }
          if (text.charCodeAt(j - 1) === 0x200d && c >= 0xd800 && c <= 0xdbff) { j += 2; continue; }
          break;
        }
        ch = text.substring(i, j);
        i = j - 1;
      }
      out.push(unescaped ? ch : ch === ">" ? "&gt;" : ch === "<" ? "&lt;" :
        preserveSpaces && ch === " " && (text.charAt(i - 1) === " " || text.charAt(i + 1) === " ") ? "&nbsp;" : ch);
    }
    return out;
  }
  function textSplit(el, delimiter, preserveSpaces, svg) {
    var result = [];
    for (var node = el.firstChild; node; node = node.nextSibling) {
      if (node.nodeType === 3) {
        var s = String(node.nodeValue).replace(/^\n+/g, "");
        if (!preserveSpaces) s = s.replace(/\s+/g, " ");
        Array.prototype.push.apply(result, textChars(s, delimiter, preserveSpaces, svg));
      } else if (String(node.nodeName).toLowerCase() === "br") {
        result[result.length - 1] += "<br>";
      } else {
        result.push(node.outerHTML);
      }
    }
    if (!svg) for (var q = result.length; q--;) if (result[q] === "&") result.splice(q, 1, "&amp;");
    return result;
  }
  function textSpec(target, value) {
    if (typeof value !== "object" || value === null) value = { value: value };
    if (!("value" in value)) return null;
    var tag = String(target.tagName).toUpperCase();
    var svg = tag === "TEXT" || tag === "TSPAN";
    var delimiter = value.delimiter || "", fillChar = value.fillChar || (value.padSpace ? "&nbsp;" : "");
    var original = textSplit(target, delimiter, value.preserveSpaces, svg);
    var tmp = document.createElement("div");
    tmp.innerHTML = String(value.value);
    var text = textSplit(tmp, delimiter, value.preserveSpaces, svg);
    var from = buildingFromTween, rtl = !!value.rtl;
    if ((from || rtl) && !(rtl && from)) { var sw = original; original = text; text = sw; }
    var diff = original.length - text.length, shorter = diff < 0 ? original : text;
    for (diff = Math.abs(diff); --diff > -1;) shorter.push(fillChar);
    if (value.type === "diff") {
      var ct = [], co = [], agg = "";
      for (var i = 0; i < text.length; i++) {
        if (text[i] === original[i]) agg += text[i];
        else { ct.push(agg + text[i]); co.push(agg + original[i]); agg = ""; }
      }
      if (agg) { ct.push(agg); co.push(agg); }
      text = ct; original = co;
    }
    if (value.speed) warn("text: speed/maxDuration are ignored (the tween keeps its duration)");
    var specs = [];
    try { specs = JSON.parse(target.getAttribute("data-uv-text") || "[]"); } catch (err) { specs = []; }
    specs.push({ text: text, original: original, delimiter: delimiter, fillChar: fillChar, from: from, rtl: rtl, svg: svg,
      newClass: (rtl ? value.oldClass : value.newClass) || "", oldClass: (rtl ? value.newClass : value.oldClass) || "" });
    target.setAttribute("data-uv-text", JSON.stringify(specs));
    return "--uv-text-" + (specs.length - 1);
  }

  function morphSpec(target, value) {
    if (!value) return null;
    if (typeof value === "string" || isElement(value) || (value && value[0] !== undefined && !value.shape)) value = { shape: value };
    if (String(target.tagName).toLowerCase() !== "path") { warn("morphSVG: only <path> targets are supported (convert shapes to paths)"); return null; }
    var shape = value.shape || value.d || "";
    var isStr = typeof shape === "string";
    if (!isStr || /(^[#.][a-z]|[a-y][a-z])/i.test(shape) || (shape.match(/[-+=.]*\d+[.e\-+]*\d*[e\-+]*\d*/gi) || []).length < 3) {
      var e = isStr ? document.querySelector(shape) : (isElement(shape) ? shape : shape && shape[0]);
      if (!isElement(e) || String(e.tagName).toLowerCase() !== "path") { warn("morphSVG: shapes must be <path> elements or path data"); return null; }
      shape = (e === target && e.getAttribute("data-original")) || e.getAttribute("d") || "";
    }
    if ((value.type && value.type !== "linear") || value.smooth || value.curveMode) warn("morphSVG: rotational/smooth morphs render as linear");
    if (!target.getAttribute("data-original")) target.setAttribute("data-original", target.getAttribute("d") || "");
    var specs = [];
    try { specs = JSON.parse(target.getAttribute("data-uv-morph") || "[]"); } catch (err) { specs = []; }
    var si = value.shapeIndex;
    specs.push({
      shape: String(shape), shapeIndex: si == null || si === "auto" ? null : si === "reverse" ? -1 : +si,
      map: value.map == null ? "size" : String(value.map), precision: isNaN(value.precision) ? 2 : +value.precision,
    });
    target.setAttribute("data-uv-morph", JSON.stringify(specs));
    return "--uv-morph-" + (specs.length - 1);
  }

  // ---- DrawSVG: the plugin's measuring and parsing, reimplemented (no GSAP code ships) -------
  function round4(v) { return Math.round(v * 10000) / 10000; }
  function drawLength(el) {
    var tag = String(el.tagName || "").toLowerCase();
    var dist = function (x1, y1, x2, y2) { return Math.sqrt((x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1)); };
    if (tag === "path") return el.getTotalLength() || 0;
    if (tag === "rect") return numAttr(el, "width") * 2 + numAttr(el, "height") * 2;
    if (tag === "line") return dist(numAttr(el, "x1"), numAttr(el, "y1"), numAttr(el, "x2"), numAttr(el, "y2"));
    if (tag === "polyline" || tag === "polygon") {
      var p = String(el.getAttribute("points") || "").match(/[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?/g) || [];
      if (tag === "polygon") p.push(p[0], p[1]);
      var len = 0;
      for (var i = 2; i < p.length; i += 2) len += dist(+p[i - 2], +p[i - 1], +p[i], +p[i + 1]) || 0;
      return len;
    }
    if (tag === "circle" || tag === "ellipse") {
      var rx = tag === "circle" ? numAttr(el, "r") : numAttr(el, "rx"), ry = tag === "circle" ? rx : numAttr(el, "ry");
      return Math.PI * (3 * (rx + ry) - Math.sqrt((3 * rx + ry) * (rx + 3 * ry)));
    }
    return 0;
  }
  function drawSingle(value, length) {
    var n = parseFloat(value) || 0;
    return String(value).indexOf("%") !== -1 ? n / 100 * length : n;
  }
  function drawSVGValue(el, value) {
    var length = drawLength(el);
    value = value === true || value + "" === "true" ? "0 100%" : !value ? "0 0" : (value + "").indexOf(" ") === -1 ? "0 " + value : value + "";
    var sp = value.indexOf(" "), s = drawSingle(value.substr(0, sp), length), e = drawSingle(value.substr(sp + 1), length);
    if (s > e) { var tmp = s; s = e; e = tmp; }
    return round4(e - s) + " " + round4(-s) + " " + round4(length);
  }
  // golden tests (src/scene.rs) check this against DrawSVGPlugin's own output
  g.__uvDrawSVGValue = drawSVGValue;
  function drawSVGBase(el) {
    var length = drawLength(el), cs = getComputedStyle(el);
    var dash = String((cs && cs.strokeDasharray) || el.getAttribute("stroke-dasharray") || "");
    var offset = parseFloat((cs && cs.strokeDashoffset) || el.getAttribute("stroke-dashoffset")) || 0;
    var i = dash.indexOf(",");
    if (i < 0) i = dash.indexOf(" ");
    var d = i < 0 ? length : parseFloat(dash.substr(0, i)) || 0;
    if (d > length) d = length;
    var s = -offset || 0, e = (d - offset) || 0;
    return round4(e - s) + " " + round4(-s) + " " + round4(length);
  }
  function polyLength(polys) {
    var len = 0;
    polys.forEach(function (p) { for (var i = 1; i < p.length; i++) len += Math.hypot(p[i][0] - p[i - 1][0], p[i][1] - p[i - 1][1]); });
    return len;
  }
  // A <path>'s length and points the way Blink measures them (native binding "pathMeasure");
  // null where the build has no Skia
  function nativePathMeasure(el, lengths) {
    if (String(el.tagName).toLowerCase() !== "path") return null;
    try {
      var r = __blitz_native("pathMeasure", JSON.stringify([String(el.getAttribute("d") || ""), lengths || []]));
      return r ? JSON.parse(r) : null;
    } catch (e) { return null; }
  }
  // Web APIs registry scripts use that Boa/Blitz don't provide (small, guarded stand-ins)
  if (!g.URLSearchParams) {
    g.URLSearchParams = function (init) {
      var self = this;
      this._p = [];
      if (init && typeof init === "object") {
        for (var k in init) if (Object.prototype.hasOwnProperty.call(init, k)) this._p.push([k, String(init[k])]);
        return;
      }
      var dec = function (x) { try { return decodeURIComponent(x.replace(/\+/g, " ")); } catch (e) { return x; } };
      String(init == null ? "" : init).replace(/^\?/, "").split("&").forEach(function (kv) {
        if (!kv) return;
        var i = kv.indexOf("=");
        self._p.push([dec(i < 0 ? kv : kv.slice(0, i)), dec(i < 0 ? "" : kv.slice(i + 1))]);
      });
    };
    var USP = g.URLSearchParams.prototype;
    USP.get = function (k) { for (var i = 0; i < this._p.length; i++) if (this._p[i][0] === k) return this._p[i][1]; return null; };
    USP.getAll = function (k) { return this._p.filter(function (p) { return p[0] === k; }).map(function (p) { return p[1]; }); };
    USP.has = function (k) { return this.get(k) !== null; };
    USP.append = function (k, v) { this._p.push([String(k), String(v)]); };
    USP["delete"] = function (k) { this._p = this._p.filter(function (p) { return p[0] !== k; }); };
    USP.set = function (k, v) { this["delete"](k); this.append(k, v); };
    USP.forEach = function (fn, t) { var self = this; this._p.forEach(function (p) { fn.call(t, p[1], p[0], self); }); };
    USP.toString = function () { return this._p.map(function (p) { return encodeURIComponent(p[0]) + "=" + encodeURIComponent(p[1]); }).join("&"); };
    USP.entries = function () { return this._p.map(function (p) { return [p[0], p[1]]; })[Symbol.iterator](); };
    USP.keys = function () { return this._p.map(function (p) { return p[0]; })[Symbol.iterator](); };
    USP.values = function () { return this._p.map(function (p) { return p[1]; })[Symbol.iterator](); };
    USP[Symbol.iterator] = USP.entries;
  }
  if (!g.AbortController) {
    g.AbortSignal = function () { this.aborted = false; this.reason = undefined; this.onabort = null; this._l = []; };
    g.AbortSignal.prototype.addEventListener = function (t, f) { if (t === "abort") this._l.push(f); };
    g.AbortSignal.prototype.removeEventListener = function (t, f) { this._l = this._l.filter(function (x) { return x !== f; }); };
    g.AbortSignal.prototype.throwIfAborted = function () { if (this.aborted) throw this.reason; };
    g.AbortController = function () { this.signal = new g.AbortSignal(); };
    g.AbortController.prototype.abort = function (reason) {
      var s = this.signal;
      if (s.aborted) return;
      s.aborted = true;
      s.reason = reason;
      var ev = { type: "abort", target: s };
      if (s.onabort) s.onabort(ev);
      s._l.slice().forEach(function (f) { f(ev); });
    };
  }
  if (!g.NodeFilter) {
    g.NodeFilter = { FILTER_ACCEPT: 1, FILTER_REJECT: 2, FILTER_SKIP: 3, SHOW_ALL: 0xffffffff, SHOW_ELEMENT: 0x1,
      SHOW_ATTRIBUTE: 0x2, SHOW_TEXT: 0x4, SHOW_CDATA_SECTION: 0x8, SHOW_COMMENT: 0x80, SHOW_DOCUMENT: 0x100,
      SHOW_DOCUMENT_TYPE: 0x200, SHOW_DOCUMENT_FRAGMENT: 0x400 };
  }
  if (g.document && !g.document.createTreeWalker) {
    // a snapshot of the matching descendants in document order (enough for read-then-edit walks)
    g.document.createTreeWalker = function (root, whatToShow, filter) {
      var show = whatToShow == null ? 0xffffffff : whatToShow >>> 0, nodes = [];
      (function walk(n) {
        var kids = n.childNodes || [];
        for (var i = 0; i < kids.length; i++) {
          var c = kids[i];
          if (show & (1 << ((c.nodeType || 1) - 1))) {
            var r = !filter ? 1 : typeof filter === "function" ? filter(c) : filter.acceptNode(c);
            if (r === 1) nodes.push(c);
            if (r === 2) continue;
          }
          walk(c);
        }
      })(root);
      var i = -1;
      return {
        root: root, currentNode: root, whatToShow: show, filter: filter || null,
        nextNode: function () { if (i + 1 < nodes.length) { this.currentNode = nodes[++i]; return this.currentNode; } return null; },
        previousNode: function () { if (i > 0) { this.currentNode = nodes[--i]; return this.currentNode; } return null; },
      };
    };
  }
  if (!g.matchMedia) {
    // headless Chrome: no reduced motion, light scheme; width/height/orientation against the viewport
    g.matchMedia = function (q) {
      q = String(q);
      var w = g.innerWidth || 0, h = g.innerHeight || 0, m = true;
      q.replace(/\(\s*(min|max)-(width|height)\s*:\s*([\d.]+)px\s*\)/g, function (_, mm, dim, v) {
        var x = dim === "width" ? w : h;
        if (mm === "min" ? x < +v : x > +v) m = false;
      });
      if (/orientation\s*:\s*portrait/.test(q) && w > h) m = false;
      if (/orientation\s*:\s*landscape/.test(q) && h > w) m = false;
      if (/prefers-reduced-motion\s*:\s*reduce|prefers-color-scheme\s*:\s*dark|prefers-contrast\s*:\s*more|hover\s*:\s*none|pointer\s*:\s*coarse/.test(q)) m = false;
      return { matches: m, media: q, onchange: null, addListener: function () {}, removeListener: function () {},
        addEventListener: function () {}, removeEventListener: function () {}, dispatchEvent: function () { return true; } };
    };
  }
  if (g.document && !g.document.importNode) {
    g.document.importNode = function (n, deep) { return n.cloneNode(!!deep); };
  }
  if (g.Element && !g.Element.prototype.insertAdjacentHTML) {
    g.Element.prototype.insertAdjacentHTML = function (pos, html) {
      var tmp = g.document.createElement("div");
      tmp.innerHTML = html;
      var kids = Array.prototype.slice.call(tmp.childNodes), where = String(pos).toLowerCase(), self = this;
      var parent = this.parentNode, first = this.firstChild, next = this.nextSibling;
      kids.forEach(function (k) {
        if (where === "beforebegin") { if (parent) parent.insertBefore(k, self); }
        else if (where === "afterbegin") self.insertBefore(k, first);
        else if (where === "afterend") { if (parent) parent.insertBefore(k, next); }
        else self.appendChild(k);
      });
    };
  }
  (function () {
    try {
      var st = g.document && g.document.documentElement && g.document.documentElement.style;
      var SP = st && Object.getPrototypeOf(st);
      if (SP && !SP.getPropertyPriority) SP.getPropertyPriority = function () { return ""; };
      // CSSOM: setProperty(name, "") removes the declaration (scripts restore "no inline value" that way)
      if (SP && SP.setProperty && SP.removeProperty && !SP.setProperty.__uvEmpty) {
        var setProp = SP.setProperty;
        SP.setProperty = function (name, value, priority) {
          if (value === "" || value == null) return this.removeProperty(name);
          return setProp.apply(this, arguments);
        };
        SP.setProperty.__uvEmpty = true;
      }
    } catch (e) {}
    try {
      var frag = g.document && g.document.createDocumentFragment && g.document.createDocumentFragment();
      var FP = frag && Object.getPrototypeOf(frag);
      if (FP && !FP.getElementById) FP.getElementById = function (id) {
        return this.querySelector ? this.querySelector('[id="' + String(id).replace(/["\\]/g, "\\$&") + '"]') : null;
      };
    } catch (e) {}
  })();
  var SVG_NS = "http://www.w3.org/2000/svg";
  function matMul(m, n) {
    return [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3],
      m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]];
  }
  function svgMatrix(m) { return { a: m[0], b: m[1], c: m[2], d: m[3], e: m[4], f: m[5] }; }
  function svgPoint(p) {
    return { x: p.x, y: p.y, matrixTransform: function (m) {
      return svgPoint({ x: m.a * p.x + m.c * p.y + m.e, y: m.b * p.x + m.d * p.y + m.f });
    } };
  }
  // an SVG transform attribute as a list of [a b c d e f] matrices, in order
  function svgTransformList(text) {
    var out = [], re = /(matrix|translate|scale|rotate|skewX|skewY)\s*\(([^)]*)\)/g, m;
    while ((m = re.exec(String(text || "")))) {
      var v = m[2].trim().split(/[\s,]+/).filter(Boolean).map(Number), r;
      switch (m[1]) {
        case "matrix": out.push(v.length === 6 ? v : [1, 0, 0, 1, 0, 0]); break;
        case "translate": out.push([1, 0, 0, 1, v[0] || 0, v[1] || 0]); break;
        case "scale": out.push([v[0], 0, 0, v.length > 1 ? v[1] : v[0], 0, 0]); break;
        case "rotate":
          r = (v[0] || 0) * Math.PI / 180;
          var rot = [Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0];
          out.push(v.length >= 3 ? matMul(matMul([1, 0, 0, 1, v[1], v[2]], rot), [1, 0, 0, 1, -v[1], -v[2]]) : rot);
          break;
        case "skewX": out.push([1, 0, Math.tan((v[0] || 0) * Math.PI / 180), 1, 0, 0]); break;
        case "skewY": out.push([1, Math.tan((v[0] || 0) * Math.PI / 180), 0, 1, 0, 0]); break;
      }
    }
    return out;
  }
  // XMLSerializer (markup of a node, as outerHTML gives it)
  if (!g.XMLSerializer) g.XMLSerializer = function () {};
  if (!g.XMLSerializer.prototype.serializeToString) g.XMLSerializer.prototype.serializeToString = function (node) {
    if (!node) return "";
    if (node.outerHTML !== undefined) return node.outerHTML;
    if (node.nodeType === 9 && node.documentElement) return node.documentElement.outerHTML;
    if (node.childNodes) { var o = ""; for (var i = 0; i < node.childNodes.length; i++) o += this.serializeToString(node.childNodes[i]); return o; }
    return String(node.textContent || "");
  };
  // Blitz gives SVG elements the plain Element prototype
  [g.Element && g.Element.prototype, g.SVGElement && g.SVGElement.prototype].forEach(function (svgProto) {
    if (!svgProto) return;
    if (!("viewBox" in svgProto)) Object.defineProperty(svgProto, "viewBox", {
      configurable: true,
      get: function () {
        var v = String(this.getAttribute("viewBox") || "").trim().split(/[\s,]+/).map(Number);
        var r = v.length === 4 && v.every(isFinite) ? { x: v[0], y: v[1], width: v[2], height: v[3] } : { x: 0, y: 0, width: 0, height: 0 };
        return { baseVal: r, animVal: r };
      },
    });
    if (!svgProto.getTotalLength) svgProto.getTotalLength = function () {
      var m = nativePathMeasure(this);
      if (m) return m.length;
      if (String(this.tagName).toLowerCase() === "circle") return 2 * Math.PI * numAttr(this, "r");
      return polyLength(shapePolys(this));
    };
    if (!svgProto.getPointAtLength) svgProto.getPointAtLength = function (target) {
      return svgPoint(pointAtLength(this, target));
    };
    function pointAtLength(el, target) {
      var m = nativePathMeasure(el, [+target || 0]);
      if (m && m.points.length) return { x: m.points[0][0], y: m.points[0][1] };
      var polys = shapePolys(el), acc = 0, last = { x: 0, y: 0 };
      for (var q = 0; q < polys.length; q++) {
        var p = polys[q];
        for (var i = 1; i < p.length; i++) {
          var seg = Math.hypot(p[i][0] - p[i - 1][0], p[i][1] - p[i - 1][1]);
          if (acc + seg >= target && seg > 0) {
            var f = (target - acc) / seg;
            return { x: p[i - 1][0] + (p[i][0] - p[i - 1][0]) * f, y: p[i - 1][1] + (p[i][1] - p[i - 1][1]) * f };
          }
          acc += seg;
          last = { x: p[i][0], y: p[i][1] };
        }
      }
      return last;
    }
    // the nearest <svg> above an SVG element (null for an outermost <svg> and for HTML)
    if (!("ownerSVGElement" in svgProto)) Object.defineProperty(svgProto, "ownerSVGElement", {
      configurable: true,
      get: function () {
        if (this.namespaceURI !== SVG_NS) return null;
        for (var n = this.parentNode; n && n.nodeType === 1; n = n.parentNode) {
          if (n.namespaceURI === SVG_NS && String(n.localName || n.tagName).toLowerCase() === "svg") return n;
        }
        return null;
      },
    });
    // SVG `transform` (SVGAnimatedTransformList): consolidate() folds the attribute into one matrix
    if (!("transform" in svgProto)) Object.defineProperty(svgProto, "transform", {
      configurable: true,
      get: function () {
        if (this.namespaceURI !== SVG_NS) return undefined;
        var el = this;
        var list = {
          get numberOfItems() { return svgTransformList(el.getAttribute("transform")).length; },
          consolidate: function () {
            var items = svgTransformList(el.getAttribute("transform"));
            if (!items.length) return null;
            var m = items.reduce(function (acc, t) { return matMul(acc, t); }, [1, 0, 0, 1, 0, 0]);
            return { type: 1, angle: 0, matrix: svgMatrix(m) };
          },
        };
        return { baseVal: list, animVal: list };
      },
    });
    // SVG <text> has no layout box (Blitz paints the SVG as one image): measure the same font in an
    // off-screen span, place it by x/y and text-anchor, ascent ~80% of the line box.
    function textBBox(el) {
      var cs = getComputedStyle(el), span = document.createElement("span");
      var size = el.getAttribute("font-size"), weight = el.getAttribute("font-weight");
      span.style.cssText = "position:absolute;left:-10000px;top:0;white-space:pre;font-family:" + cs.fontFamily +
        ";font-size:" + (size ? parseFloat(size) + "px" : cs.fontSize) + ";font-weight:" + (weight || cs.fontWeight) +
        ";font-style:" + cs.fontStyle + ";letter-spacing:" + cs.letterSpacing;
      span.textContent = el.textContent;
      document.body.appendChild(span);
      var r = span.getBoundingClientRect();
      span.remove();
      var x = parseFloat(el.getAttribute("x")) || 0, y = parseFloat(el.getAttribute("y")) || 0;
      var anchor = el.getAttribute("text-anchor") || cs.textAnchor;
      var left = anchor === "middle" ? x - r.width / 2 : anchor === "end" ? x - r.width : x;
      return { x: left, y: y - r.height * 0.8, width: r.width, height: r.height };
    }
    if (!svgProto.getBBox) svgProto.getBBox = function () {
      var polys = shapePolys(this);
      if (!polys.length && (this.localName === "text" || this.localName === "tspan")) return textBBox(this);
      if (!polys.length) {
        var kids = this.children || [], box = null;
        for (var k = 0; k < kids.length; k++) {
          if (!kids[k].getBBox) continue;
          var b = kids[k].getBBox();
          if (!b.width && !b.height) continue;
          box = box ? { x: Math.min(box.x, b.x), y: Math.min(box.y, b.y), r: Math.max(box.r, b.x + b.width), b: Math.max(box.b, b.y + b.height) } : { x: b.x, y: b.y, r: b.x + b.width, b: b.y + b.height };
        }
        return box ? { x: box.x, y: box.y, width: box.r - box.x, height: box.b - box.y } : { x: 0, y: 0, width: 0, height: 0 };
      }
      var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      polys.forEach(function (p) { p.forEach(function (pt) { minX = Math.min(minX, pt[0]); minY = Math.min(minY, pt[1]); maxX = Math.max(maxX, pt[0]); maxY = Math.max(maxY, pt[1]); }); });
      return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
    };
  });

  // ---- Loading APIs (M5): binary fetch, XMLHttpRequest, Image, observers, Intl gaps ---------
  // Bytes come through the script fetcher's "uv-bytes:" channel (src/scene.rs NetFetcher: base64).
  function b64ToBytes(b64) {
    var map = {}, chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for (var c = 0; c < 64; c++) map[chars.charAt(c)] = c;
    var clean = String(b64).replace(/[^A-Za-z0-9+\/]/g, ""), out = new Uint8Array(Math.floor(clean.length * 3 / 4)), o = 0;
    for (var i = 0; i < clean.length; i += 4) {
      var a = map[clean.charAt(i)], b = map[clean.charAt(i + 1)], cc = map[clean.charAt(i + 2)], d = map[clean.charAt(i + 3)];
      out[o++] = (a << 2) | (b >> 4);
      if (cc !== undefined && o < out.length) out[o++] = ((b & 15) << 4) | (cc >> 2);
      if (d !== undefined && o < out.length) out[o++] = ((cc & 3) << 6) | d;
    }
    return o < out.length ? out.slice(0, o) : out;
  }
  function resolveUrl(u) { return __blitz_fetch_sync(String(u))[1]; }
  // High Resolution Time + User Timing (Tailwind's browser build marks every build step; a throw
  // there leaves its stylesheet empty)
  if (typeof g.performance !== "object" || !g.performance) {
    var perfOrigin = Date.now();
    g.performance = { timeOrigin: perfOrigin, now: function () { return Date.now() - perfOrigin; }, toJSON: function () { return {}; } };
  }
  if (typeof g.performance === "object" && g.performance) {
    var perfEntries = [];
    if (typeof performance.mark !== "function") performance.mark = function (name, opts) { var e = { name: String(name), entryType: "mark", startTime: performance.now ? performance.now() : 0, duration: 0, detail: opts && opts.detail }; perfEntries.push(e); return e; };
    if (typeof performance.measure !== "function") performance.measure = function (name) { var e = { name: String(name), entryType: "measure", startTime: 0, duration: 0 }; perfEntries.push(e); return e; };
    if (typeof performance.clearMarks !== "function") performance.clearMarks = function () { perfEntries = perfEntries.filter(function (e) { return e.entryType !== "mark"; }); };
    if (typeof performance.clearMeasures !== "function") performance.clearMeasures = function () { perfEntries = perfEntries.filter(function (e) { return e.entryType !== "measure"; }); };
    if (typeof performance.getEntriesByName !== "function") performance.getEntriesByName = function (n) { return perfEntries.filter(function (e) { return e.name === n; }); };
    if (typeof performance.getEntriesByType !== "function") performance.getEntriesByType = function (t) { return perfEntries.filter(function (e) { return e.entryType === t; }); };
    if (typeof performance.getEntries !== "function") performance.getEntries = function () { return perfEntries.slice(); };
  }
  function fetchBytes(absUrl) {
    var r = __blitz_fetch_sync("uv-bytes:" + absUrl);
    if (r[0] !== 200) throw new TypeError("Failed to fetch " + absUrl);
    return b64ToBytes(r[2]);
  }
  if (typeof g.Blob !== "function") {
    g.Blob = function (parts, opts) {
      var chunks = [], size = 0;
      (parts || []).forEach(function (p) {
        var b = typeof p === "string" ? new TextEncoder().encode(p) : p instanceof ArrayBuffer ? new Uint8Array(p) : p && p._bytes ? p._bytes : new Uint8Array(p.buffer || p);
        chunks.push(b); size += b.length;
      });
      var all = new Uint8Array(size), at = 0;
      chunks.forEach(function (b) { all.set(b, at); at += b.length; });
      this._bytes = all; this.size = size; this.type = (opts && opts.type) || "";
    };
    g.Blob.prototype.arrayBuffer = function () { return Promise.resolve(this._bytes.slice().buffer); };
    g.Blob.prototype.text = function () { return Promise.resolve(new TextDecoder().decode(this._bytes)); };
  }
  // Object URLs are data: URLs, so <img> (Blitz's loader), imageSize and fetch read them like any other
  // URL (three.js's GLTFLoader puts embedded textures in blobs).
  if (typeof g.URL === "function" && typeof g.URL.createObjectURL !== "function") {
    g.URL.createObjectURL = function (blob) {
      var bytes = blob && blob._bytes ? blob._bytes : new Uint8Array(0);
      return "data:" + ((blob && blob.type) || "application/octet-stream") + ";base64," + bufferToBase64(bytes);
    };
    g.URL.revokeObjectURL = function () {};
  }
  if (typeof g.Request !== "function") {
    // fetch() reads `url` from any object; three.js's FileLoader wraps every load in a Request
    g.Request = function (input, init) {
      init = init || {};
      var from = input && typeof input === "object" ? input : null;
      this.url = String(from ? from.url : input);
      this.method = String(init.method || (from && from.method) || "GET").toUpperCase();
      this.headers = new Headers(init.headers || (from && from.headers) || undefined);
      this.credentials = init.credentials || (from && from.credentials) || "same-origin";
      this.mode = init.mode || "cors"; this.cache = init.cache || "default"; this.redirect = "follow";
      this.signal = init.signal || null; this.body = null; this.bodyUsed = false;
    };
    g.Request.prototype.clone = function () { return new g.Request(this); };
  }
  if (g.Response && g.Response.prototype) {
    var respText = g.Response.prototype.text;
    g.Response.prototype.arrayBuffer = function () {
      var self = this;
      if (self.url) { try { return Promise.resolve(fetchBytes(self.url).buffer); } catch (e) { /* fall back to the text body */ } }
      return respText.call(self).then(function (t) { return new TextEncoder().encode(t).buffer; });
    };
    g.Response.prototype.blob = function () { return this.arrayBuffer().then(function (b) { return new Blob([b]); }); };
    g.Response.prototype.bytes = function () { return this.arrayBuffer().then(function (b) { return new Uint8Array(b); }); };
  }
  if (typeof g.XMLHttpRequest !== "function") {
    var XHR = function () {
      this.readyState = 0; this.status = 0; this.statusText = ""; this.responseText = ""; this.response = null;
      this.responseType = ""; this.responseURL = ""; this.timeout = 0; this.withCredentials = false; this._listeners = {};
    };
    ["UNSENT", "OPENED", "HEADERS_RECEIVED", "LOADING", "DONE"].forEach(function (n, i) { XHR[n] = i; XHR.prototype[n] = i; });
    XHR.prototype.open = function (method, url, async) { this._url = String(url); this._async = async !== false; this.readyState = 1; };
    XHR.prototype.setRequestHeader = function () {};
    XHR.prototype.overrideMimeType = function () {};
    XHR.prototype.abort = function () {};
    XHR.prototype.getAllResponseHeaders = function () { return ""; };
    XHR.prototype.getResponseHeader = function () { return null; };
    XHR.prototype.addEventListener = function (t, f) { (this._listeners[t] = this._listeners[t] || []).push(f); };
    XHR.prototype.removeEventListener = function (t, f) { var l = this._listeners[t] || []; var k = l.indexOf(f); if (k >= 0) l.splice(k, 1); };
    XHR.prototype._fire = function (t) {
      var ev = { type: t, target: this, currentTarget: this, loaded: 0, total: 0, lengthComputable: false };
      if (typeof this["on" + t] === "function") this["on" + t].call(this, ev);
      var self = this;
      (this._listeners[t] || []).slice().forEach(function (f) { f.call(self, ev); });
    };
    XHR.prototype.send = function () {
      var self = this;
      var run = function () {
        try {
          var r = __blitz_fetch_sync(self._url), t = self.responseType;
          self.status = r[0]; self.statusText = r[0] === 200 ? "OK" : r[0] === 404 ? "Not Found" : ""; self.responseURL = r[1];
          if (t === "arraybuffer" || t === "blob") {
            var bytes = fetchBytes(r[1]);
            self.response = t === "blob" ? new Blob([bytes]) : bytes.buffer;
          } else if (t === "json") {
            try { self.response = JSON.parse(r[2]); } catch (e) { self.response = null; }
          } else {
            self.responseText = r[2]; self.response = r[2];
          }
          self.readyState = 4;
          self._fire("readystatechange"); self._fire("load"); self._fire("loadend");
        } catch (e) {
          self.readyState = 4; self.status = 0;
          self._fire("readystatechange"); self._fire("error"); self._fire("loadend");
        }
      };
      if (this._async) setTimeout(run, 0); else run();
    };
    g.XMLHttpRequest = XHR;
  }
  // Intrinsic size from the file header (PNG, GIF, JPEG, WebP) or an SVG's width/height/viewBox.
  function imageSize(absUrl) {
    try {
      var b = fetchBytes(absUrl), n = b.length;
      var be16 = function (i) { return (b[i] << 8) | b[i + 1]; }, le16 = function (i) { return b[i] | (b[i + 1] << 8); };
      if (n > 24 && b[0] === 0x89 && b[1] === 0x50) return [(b[16] << 24 | b[17] << 16 | b[18] << 8 | b[19]) >>> 0, (b[20] << 24 | b[21] << 16 | b[22] << 8 | b[23]) >>> 0];
      if (n > 10 && b[0] === 0x47 && b[1] === 0x49) return [le16(6), le16(8)];
      if (n > 4 && b[0] === 0xff && b[1] === 0xd8) {
        for (var i = 2; i + 9 < n;) {
          if (b[i] !== 0xff) { i++; continue; }
          var m = b[i + 1];
          if (m >= 0xc0 && m <= 0xcf && m !== 0xc4 && m !== 0xc8 && m !== 0xcc) return [be16(i + 7), be16(i + 5)];
          i += 2 + be16(i + 2);
        }
      }
      if (n > 30 && b[8] === 0x57 && b[9] === 0x45 && b[10] === 0x42 && b[11] === 0x50) {
        var kind = String.fromCharCode(b[12], b[13], b[14], b[15]);
        if (kind === "VP8 ") return [le16(26) & 0x3fff, le16(28) & 0x3fff];
        if (kind === "VP8L") { var bits = b[21] | (b[22] << 8) | (b[23] << 16) | (b[24] << 24); return [(bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1]; }
        if (kind === "VP8X") return [1 + (b[24] | (b[25] << 8) | (b[26] << 16)), 1 + (b[27] | (b[28] << 8) | (b[29] << 16))];
      }
      var text = new TextDecoder().decode(b.slice(0, 4096)), tag = text.match(/<svg[^>]*>/i);
      if (tag) {
        var w = /\swidth\s*=\s*["']?([\d.]+)(px)?["'\s>]/i.exec(tag[0]), h = /\sheight\s*=\s*["']?([\d.]+)(px)?["'\s>]/i.exec(tag[0]);
        var vb = /viewBox\s*=\s*["']\s*[-\d.]+[\s,]+[-\d.]+[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(tag[0]);
        if (w && h) return [+w[1], +h[1]];
        if (vb && w) return [+w[1], +w[1] * vb[2] / vb[1]];
        if (vb && h) return [+h[1] * vb[1] / vb[2], +h[1]];
        return [300, 150];
      }
    } catch (e) { /* unknown */ }
    return [0, 0];
  }
  // DOMTokenList: iterable, with entries/keys/values/replace/supports
  (function classLists() {
    var proto = Object.getPrototypeOf(document.documentElement), d = proto && Object.getOwnPropertyDescriptor(proto, "classList");
    if (!d || !d.get) return;
    Object.defineProperty(proto, "classList", {
      configurable: true,
      get: function () {
        var el = this, list = d.get.call(el);
        var names = function () { return String(el.getAttribute("class") || "").split(/\s+/).filter(Boolean); };
        if (typeof Symbol === "function" && !list[Symbol.iterator]) list[Symbol.iterator] = function () { return names()[Symbol.iterator](); };
        if (!list.values) list.values = function () { return names()[Symbol.iterator](); };
        if (!list.keys) list.keys = function () { return names().keys(); };
        if (!list.entries) list.entries = function () { return names().entries(); };
        if (!list.replace) list.replace = function (a, b) { var n = names(), k = n.indexOf(String(a)); if (k < 0) return false; n[k] = String(b); el.setAttribute("class", n.join(" ")); return true; };
        if (!list.supports) list.supports = function () { return true; };
        return list;
      },
    });
  })();
  (function events() {
    // Blitz's Event/CustomEvent are illegal-constructor stubs and nodes have no dispatchEvent, so script-made
    // events (three.js's ImageLoader waits on an <img> "load" listener; libraries' custom events) dispatch
    // through JS-side listener lists here. Native events (clicks) still go through Blitz's own registry.
    var Ev = function Event(type, init) {
      if (arguments.length < 1) throw new TypeError("Failed to construct 'Event': 1 argument required, but only 0 present.");
      init = init || {};
      this.type = String(type); this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable; this.composed = !!init.composed;
      this.defaultPrevented = false; this.target = this.currentTarget = this.srcElement = null; this.eventPhase = 0;
      this.isTrusted = false; this.timeStamp = g.performance && performance.now ? performance.now() : 0; this._uvStop = 0;
    };
    Ev.NONE = 0; Ev.CAPTURING_PHASE = 1; Ev.AT_TARGET = 2; Ev.BUBBLING_PHASE = 3;
    Ev.prototype.preventDefault = function () { if (this.cancelable) this.defaultPrevented = true; };
    Ev.prototype.stopPropagation = function () { this._uvStop = Math.max(this._uvStop, 1); };
    Ev.prototype.stopImmediatePropagation = function () { this._uvStop = 2; };
    Ev.prototype.composedPath = function () { var p = []; for (var n = this.target; n; n = n.parentNode) p.push(n); return p; };
    Ev.prototype.initEvent = function (type, bubbles, cancelable) { this.type = String(type); this.bubbles = !!bubbles; this.cancelable = !!cancelable; };
    var CEv = function CustomEvent(type, init) { Ev.apply(this, arguments); this.detail = init && init.detail !== undefined ? init.detail : null; };
    CEv.prototype = Object.create(Ev.prototype); CEv.prototype.constructor = CEv;
    CEv.prototype.initCustomEvent = function (type, bubbles, cancelable, detail) { this.initEvent(type, bubbles, cancelable); this.detail = detail; };
    g.Event = Ev; g.CustomEvent = CEv;
    var capture = function (o) { return !!(o === true || (o && typeof o === "object" && o.capture)); };
    var wrap = function (target) {
      if (!target || target._uvEvents) return;
      Object.defineProperty(target, "_uvEvents", { value: true, configurable: true });
      var add = target.addEventListener, remove = target.removeEventListener;
      target.addEventListener = function (type, fn, opts) {
        var self = this == null ? g : this; // bare addEventListener(...) calls target the window
        if (fn) {
          if (!self._uvL) Object.defineProperty(self, "_uvL", { value: {}, configurable: true, writable: true });
          var list = self._uvL[type] = self._uvL[type] || [], c = capture(opts);
          if (!list.some(function (e) { return e.fn === fn && e.capture === c; })) list.push({ fn: fn, capture: c, once: !!(opts && typeof opts === "object" && opts.once) });
        }
        return add ? add.apply(self, arguments) : undefined;
      };
      target.removeEventListener = function (type, fn, opts) {
        var self = this == null ? g : this, list = self._uvL && self._uvL[type], c = capture(opts);
        if (list) for (var i = list.length - 1; i >= 0; i--) if (list[i].fn === fn && list[i].capture === c) list.splice(i, 1);
        return remove ? remove.apply(self, arguments) : undefined;
      };
      target.dispatchEvent = function (ev) {
        if (!ev || typeof ev.type !== "string") throw new TypeError("Failed to execute 'dispatchEvent': parameter 1 is not of type 'Event'.");
        var self = this == null ? g : this;
        ev.target = ev.srcElement = self;
        var path = [self];
        if (ev.bubbles && self !== g) {
          for (var n = self.parentNode; n; n = n.parentNode) path.push(n);
          if (path[path.length - 1] === document) path.push(g);
        }
        for (var i = 0; i < path.length && !ev._uvStop; i++) {
          var node = path[i], list = ((node._uvL && node._uvL[ev.type]) || []).slice(), h = node["on" + ev.type];
          if (typeof h === "function") list.push({ fn: h });
          ev.currentTarget = node; ev.eventPhase = i === 0 ? 2 : 3;
          for (var k = 0; k < list.length && ev._uvStop < 2; k++) {
            var e = list[k];
            if (e.once) node.removeEventListener(ev.type, e.fn, { capture: e.capture });
            try {
              if (typeof e.fn === "function") e.fn.call(node, ev);
              else if (e.fn && typeof e.fn.handleEvent === "function") e.fn.handleEvent(ev);
            } catch (err) {
              setTimeout(function () { throw err; }, 0); // reported, like a browser's uncaught listener error
            }
          }
        }
        ev.currentTarget = null; ev.eventPhase = 0;
        return !ev.defaultPrevented;
      };
    };
    if (document.documentElement) wrap(Object.getPrototypeOf(document.documentElement));
    wrap(document);
    wrap(g);
  })();
  (function images() {
    var proto = Object.getPrototypeOf(document.documentElement);
    if (!proto || Object.getOwnPropertyDescriptor(proto, "naturalWidth")) return;
    var isImg = function (el) { return String(el.tagName).toLowerCase() === "img"; };
    var load = function (el) {
      var src = el.getAttribute("src");
      if (!src) return;
      el._uvComplete = false;
      // a microtask: Chrome fires a cached image's load before a fetch() body resolves
      Promise.resolve().then(function () {
        var size;
        try { size = imageSize(resolveUrl(src)); } catch (e) { size = [0, 0]; }
        el._uvNatural = size;
        el._uvComplete = true;
        el.dispatchEvent(new Event(size[0] ? "load" : "error")); // runs onload/onerror too
        (el._uvDecode || []).forEach(function (r) { r(); });
        el._uvDecode = [];
      });
    };
    Object.defineProperty(proto, "src", {
      configurable: true,
      get: function () { return this.getAttribute("src") || ""; },
      set: function (v) { this.setAttribute("src", String(v)); if (isImg(this)) load(this); },
    });
    Object.defineProperty(proto, "naturalWidth", { configurable: true, get: function () { return (this._uvNatural || [0])[0]; } });
    Object.defineProperty(proto, "naturalHeight", { configurable: true, get: function () { return (this._uvNatural || [0, 0])[1]; } });
    Object.defineProperty(proto, "complete", { configurable: true, get: function () { return this._uvComplete !== false; } });
    proto.decode = function () {
      var el = this;
      if (el._uvComplete !== false) return Promise.resolve();
      return new Promise(function (res) { (el._uvDecode = el._uvDecode || []).push(res); });
    };
    if (typeof g.Image !== "function" || g.Image === g.HTMLImageElement) {
      g.Image = function (w, h) {
        var img = document.createElement("img");
        if (w != null) img.setAttribute("width", String(w));
        if (h != null) img.setAttribute("height", String(h));
        return img;
      };
    }
  })();
  (function mutationObservers() {
    if (typeof g.MutationObserver === "function") return;
    var live = [], pending = false;
    var contains = function (a, b) { for (var n = b; n; n = n.parentNode) if (n === a) return true; return false; };
    var deliver = function () {
      pending = false;
      live.slice().forEach(function (o) {
        if (!o._records.length) return;
        var r = o._records; o._records = [];
        o._cb.call(o, r, o);
      });
    };
    var record = function (rec) {
      if (!live.length) return;
      live.forEach(function (o) {
        for (var i = 0; i < o._targets.length; i++) {
          var t = o._targets[i], opt = t.opts;
          if (!(rec.target === t.node || (opt.subtree && contains(t.node, rec.target)))) continue;
          if (rec.type === "childList" && !opt.childList) continue;
          if (rec.type === "attributes" && (!opt.attributes || (opt.attributeFilter && opt.attributeFilter.indexOf(rec.attributeName) < 0))) continue;
          if (rec.type === "characterData" && !opt.characterData) continue;
          o._records.push({
            type: rec.type, target: rec.target, addedNodes: rec.addedNodes || [], removedNodes: rec.removedNodes || [],
            previousSibling: null, nextSibling: null, attributeName: rec.attributeName || null, attributeNamespace: null,
            oldValue: (rec.type === "attributes" && opt.attributeOldValue) || (rec.type === "characterData" && opt.characterDataOldValue) ? rec.oldValue : null,
          });
          if (!pending) { pending = true; Promise.resolve().then(deliver); }
          break;
        }
      });
    };
    var MO = function (cb) { this._cb = cb; this._targets = []; this._records = []; };
    MO.prototype.observe = function (node, opts) {
      opts = Object.assign({}, opts || {});
      if (opts.attributeFilter || opts.attributeOldValue) opts.attributes = opts.attributes !== false;
      if (opts.characterDataOldValue) opts.characterData = opts.characterData !== false;
      this._targets = this._targets.filter(function (t) { return t.node !== node; });
      this._targets.push({ node: node, opts: opts });
      if (live.indexOf(this) < 0) live.push(this);
    };
    MO.prototype.disconnect = function () { this._targets = []; this._records = []; var k = live.indexOf(this); if (k >= 0) live.splice(k, 1); };
    MO.prototype.takeRecords = function () { var r = this._records; this._records = []; return r; };
    g.MutationObserver = MO;
    var nodeProto = g.Node && g.Node.prototype, elProto = Object.getPrototypeOf(document.documentElement);
    var wrap = function (proto, name, targetOf, kind) {
      var orig = proto && proto[name];
      if (typeof orig !== "function") return;
      proto[name] = function () {
        var target = targetOf(this, arguments), old = kind === "attr" ? this.getAttribute(arguments[0]) : null;
        var result = orig.apply(this, arguments);
        if (target) {
          if (kind === "attr") record({ type: "attributes", target: this, attributeName: String(arguments[0]).toLowerCase(), oldValue: old });
          else record({ type: "childList", target: target, addedNodes: [].slice.call(arguments).filter(function (n) { return n && typeof n === "object"; }) });
        }
        return result;
      };
    };
    var self = function (el) { return el; }, parent = function (el) { return el.parentNode; };
    ["appendChild", "insertBefore", "removeChild", "replaceChild", "append", "prepend"].forEach(function (n) { wrap(nodeProto, n, self); wrap(elProto, n, self); });
    ["remove", "before", "after", "replaceWith"].forEach(function (n) { wrap(nodeProto, n, parent); });
    ["setAttribute", "removeAttribute", "toggleAttribute"].forEach(function (n) { wrap(elProto, n, self, "attr"); });
    var wrapSetter = function (proto, name, kind) {
      var d = proto && Object.getOwnPropertyDescriptor(proto, name);
      if (!d || !d.set) return;
      Object.defineProperty(proto, name, {
        configurable: true, enumerable: d.enumerable, get: d.get,
        set: function (v) { d.set.call(this, v); record({ type: kind, target: this }); },
      });
    };
    wrapSetter(nodeProto, "textContent", "childList");
    wrapSetter(elProto, "innerHTML", "childList");
  })();
  if (typeof g.ResizeObserver !== "function") {
    var RO = function (cb) { this._cb = cb; this._els = []; };
    RO.prototype.observe = function (el) {
      if (this._els.indexOf(el) >= 0) return;
      this._els.push(el);
      var self = this;
      setTimeout(function () {
        if (self._els.indexOf(el) < 0) return;
        var r = el.getBoundingClientRect ? el.getBoundingClientRect() : { width: 0, height: 0 };
        var box = [{ inlineSize: r.width, blockSize: r.height }];
        self._cb.call(self, [{
          target: el, contentRect: { x: 0, y: 0, width: r.width, height: r.height, top: 0, left: 0, right: r.width, bottom: r.height },
          borderBoxSize: box, contentBoxSize: box, devicePixelContentBoxSize: box,
        }], self);
      }, 0);
    };
    RO.prototype.unobserve = function (el) { var k = this._els.indexOf(el); if (k >= 0) this._els.splice(k, 1); };
    RO.prototype.disconnect = function () { this._els = []; };
    g.ResizeObserver = RO;
  }
  if (typeof g.IntersectionObserver !== "function") {
    // a rendered composition is entirely "in view"
    var IO = function (cb) { this._cb = cb; };
    IO.prototype.observe = function (el) {
      var self = this;
      setTimeout(function () {
        var r = el.getBoundingClientRect ? el.getBoundingClientRect() : { width: 0, height: 0 };
        self._cb.call(self, [{ target: el, isIntersecting: true, intersectionRatio: 1, boundingClientRect: r, intersectionRect: r, rootBounds: null, time: 0 }], self);
      }, 0);
    };
    IO.prototype.unobserve = IO.prototype.disconnect = function () {};
    IO.prototype.takeRecords = function () { return []; };
    g.IntersectionObserver = IO;
  }
  (function intlGaps() {
    // Boa's Intl.NumberFormat lacks style "currency" and gets "percent" wrong: format those here
    if (typeof Intl === "undefined" || !Intl.NumberFormat) return;
    var Native = Intl.NumberFormat;
    var SYMBOLS = { USD: "$", EUR: "€", GBP: "£", JPY: "¥", CNY: "CN¥", INR: "₹", KRW: "₩", CAD: "CA$", AUD: "A$", NZD: "NZ$", BRL: "R$", MXN: "MX$", HKD: "HK$", ILS: "₪", VND: "₫", PHP: "₱", TWD: "NT$", XAF: "FCFA" };
    var NO_DECIMALS = { JPY: 1, KRW: 1, VND: 1, CLP: 1, ISK: 1, UGX: 1, XAF: 1, XOF: 1, PYG: 1 };
    var SUFFIX_LANGS = { de: 1, fr: 1, es: 1, it: 1, nl: 0, sv: 1, fi: 1, da: 1, nb: 1, no: 1, pl: 1, cs: 1, ru: 1, pt: 0 };
    var lang = function (locales) { return String((Array.isArray(locales) ? locales[0] : locales) || "en-US").slice(0, 2).toLowerCase(); };
    var Wrapped = function (locales, options) {
      if (!(this instanceof Wrapped)) return new Wrapped(locales, options);
      options = options || {};
      var style = options.style || "decimal";
      if (style !== "percent" && style !== "currency") return new Native(locales, options);
      var o = {};
      for (var k in options) if (k !== "style" && k !== "currency" && k !== "currencyDisplay" && k !== "currencySign") o[k] = options[k];
      var digits = style === "percent" ? 0 : NO_DECIMALS[String(options.currency).toUpperCase()] ? 0 : 2;
      if (o.minimumFractionDigits == null && o.maximumFractionDigits == null && o.maximumSignificantDigits == null) { o.minimumFractionDigits = digits; o.maximumFractionDigits = digits; }
      else if (o.maximumFractionDigits == null && o.maximumSignificantDigits == null) o.maximumFractionDigits = Math.max(o.minimumFractionDigits, digits);
      else if (o.minimumFractionDigits == null && o.maximumFractionDigits != null) o.minimumFractionDigits = Math.min(digits, o.maximumFractionDigits);
      this._style = style; this._options = options; this._locales = locales; this._inner = new Native(locales, o);
    };
    Wrapped.prototype.format = function (value) {
      var n = Number(value), l = lang(this._locales), suffix = SUFFIX_LANGS[l] === 1;
      if (this._style === "percent") return this._inner.format(n * 100) + (suffix ? " %" : "%");
      var cur = String(this._options.currency || "USD").toUpperCase(), disp = this._options.currencyDisplay;
      var sym = disp === "code" ? cur : disp === "name" ? cur : SYMBOLS[cur] || cur;
      var body = this._inner.format(Math.abs(n)), neg = n < 0 ? "-" : "";
      if (suffix) return neg + body + " " + sym;
      return neg + sym + (sym.length > 1 && /[A-Z]$/.test(sym) ? " " : "") + body;
    };
    Wrapped.prototype.formatToParts = function (v) { return [{ type: "literal", value: this.format(v) }]; };
    Wrapped.prototype.resolvedOptions = function () { var r = this._inner.resolvedOptions(); r.style = this._style; if (this._options.currency) r.currency = this._options.currency; return r; };
    Wrapped.supportedLocalesOf = Native.supportedLocalesOf;
    Intl.NumberFormat = Wrapped;
    var numLocale = Number.prototype.toLocaleString;
    Number.prototype.toLocaleString = function (locales, options) {
      if (options && (options.style === "percent" || options.style === "currency")) return new Wrapped(locales, options).format(Number(this));
      return numLocale.call(this, locales, options);
    };
  })();

  // ---- <canvas> 2D: a recording context; src/canvas.rs replays the calls on Skia -------------
  var canvasContexts = [], seekListeners = [];
  function CanvasGradient(kind, coords) { this._g = { kind: kind, coords: coords, stops: [] }; }
  CanvasGradient.prototype.addColorStop = function (o, c) { this._g.stops.push([+o, String(c)]); };
  g.CanvasGradient = CanvasGradient;
  function Path2D(arg) {
    this._ops = [];
    if (typeof arg === "string") this._ops.push(["svg", arg]);
    else if (arg && arg._ops) this._ops = arg._ops.slice();
  }
  ["moveTo", "lineTo", "quadraticCurveTo", "bezierCurveTo", "arc", "arcTo", "ellipse", "rect", "roundRect", "closePath"].forEach(function (n) {
    Path2D.prototype[n] = function () { this._ops.push([n].concat([].slice.call(arguments))); };
  });
  Path2D.prototype.addPath = function (p) { if (p && p._ops) this._ops = this._ops.concat(p._ops); };
  if (typeof g.Path2D !== "function") g.Path2D = Path2D;
  function mat(a, b, c, d, e, f) { return { a: a, b: b, c: c, d: d, e: e, f: f }; }
  function mul(m, n) {
    return mat(m.a * n.a + m.c * n.b, m.b * n.a + m.d * n.b, m.a * n.c + m.c * n.d, m.b * n.c + m.d * n.d, m.a * n.e + m.c * n.f + m.e, m.b * n.e + m.d * n.f + m.f);
  }
  var STATE_PROPS = {
    fillStyle: "#000000", strokeStyle: "#000000", lineWidth: 1, lineCap: "butt", lineJoin: "miter", miterLimit: 10,
    lineDashOffset: 0, globalAlpha: 1, globalCompositeOperation: "source-over", shadowColor: "rgba(0, 0, 0, 0)",
    shadowBlur: 0, shadowOffsetX: 0, shadowOffsetY: 0, font: "10px sans-serif", textAlign: "start",
    textBaseline: "alphabetic", direction: "ltr", imageSmoothingEnabled: true, imageSmoothingQuality: "low",
    filter: "none", letterSpacing: "0px", wordSpacing: "0px", fontKerning: "auto",
  };
  function Context2D(canvas) {
    this.canvas = canvas; this._ops = []; this._s = {}; this._stack = []; this._m = mat(1, 0, 0, 1, 0, 0); this._dash = [];
    for (var k in STATE_PROPS) this._s[k] = STATE_PROPS[k];
    canvasContexts.push(this);
  }
  Object.keys(STATE_PROPS).forEach(function (k) {
    Object.defineProperty(Context2D.prototype, k, {
      get: function () { var v = this._s[k]; return v && v._g ? v : v; },
      set: function (v) {
        if ((k === "fillStyle" || k === "strokeStyle") && v && v._g) { this._s[k] = v; this._ops.push([k, JSON.parse(JSON.stringify(v._g))]); return; }
        if ((k === "fillStyle" || k === "strokeStyle") && v && v._pattern) { this._s[k] = v; this._ops.push([k, { pattern: v._pattern }]); return; }
        if ((k === "fillStyle" || k === "strokeStyle") && v && typeof v === "object") { warn("canvas: this fill style is not supported"); return; }
        this._s[k] = typeof STATE_PROPS[k] === "number" ? +v : typeof STATE_PROPS[k] === "boolean" ? !!v : String(v);
        this._ops.push([k, this._s[k]]);
      },
    });
  });
  var C = Context2D.prototype, rec = function (n) { return function () { this._ops.push([n].concat([].slice.call(arguments))); }; };
  ["beginPath", "closePath", "moveTo", "lineTo", "quadraticCurveTo", "bezierCurveTo", "arc", "arcTo", "ellipse", "rect",
   "roundRect", "clearRect", "fillRect", "strokeRect", "fillText", "strokeText"].forEach(function (n) { C[n] = rec(n); });
  C.save = function () { this._stack.push({ s: Object.assign({}, this._s), m: this._m, dash: this._dash.slice() }); this._ops.push(["save"]); };
  C.restore = function () { var t = this._stack.pop(); if (t) { this._s = t.s; this._m = t.m; this._dash = t.dash; } this._ops.push(["restore"]); };
  C.reset = function () { this._stack = []; this._m = mat(1, 0, 0, 1, 0, 0); this._dash = []; for (var k in STATE_PROPS) this._s[k] = STATE_PROPS[k]; this._ops.push(["reset"]); };
  C.translate = function (x, y) { this._m = mul(this._m, mat(1, 0, 0, 1, +x, +y)); this._ops.push(["translate", +x, +y]); };
  C.scale = function (x, y) { this._m = mul(this._m, mat(+x, 0, 0, +y, 0, 0)); this._ops.push(["scale", +x, +y]); };
  C.rotate = function (r) { var c = Math.cos(r), s = Math.sin(r); this._m = mul(this._m, mat(c, s, -s, c, 0, 0)); this._ops.push(["rotate", +r]); };
  C.transform = function (a, b, c, d, e, f) { this._m = mul(this._m, mat(+a, +b, +c, +d, +e, +f)); this._ops.push(["transform", +a, +b, +c, +d, +e, +f]); };
  C.setTransform = function (a, b, c, d, e, f) {
    if (a && typeof a === "object") { f = a.f; e = a.e; d = a.d; c = a.c; b = a.b; a = a.a; }
    if (a === undefined) { a = 1; b = 0; c = 0; d = 1; e = 0; f = 0; }
    this._m = mat(+a, +b, +c, +d, +e, +f); this._ops.push(["setTransform", +a, +b, +c, +d, +e, +f]);
  };
  C.resetTransform = function () { this._m = mat(1, 0, 0, 1, 0, 0); this._ops.push(["resetTransform"]); };
  C.getTransform = function () { var m = this._m; return { a: m.a, b: m.b, c: m.c, d: m.d, e: m.e, f: m.f, is2D: true, isIdentity: m.a === 1 && !m.b && !m.c && m.d === 1 && !m.e && !m.f }; };
  C.setLineDash = function (d) { this._dash = (d || []).map(Number); this._ops.push(["setLineDash", this._dash.slice()]); };
  C.getLineDash = function () { return this._dash.slice(); };
  var pathCall = function (n) {
    return function (a, b) {
      if (a && a._ops) this._ops.push([n, typeof b === "string" ? b : "nonzero", a._ops.slice()]);
      else this._ops.push([n, typeof a === "string" ? a : "nonzero"]);
    };
  };
  C.fill = pathCall("fill"); C.stroke = function (p) { this._ops.push(p && p._ops ? ["stroke", "nonzero", p._ops.slice()] : ["stroke"]); }; C.clip = pathCall("clip");
  C.isPointInPath = C.isPointInStroke = function () { return false; };
  C.createLinearGradient = function (x0, y0, x1, y1) { return new CanvasGradient("linear", [+x0, +y0, +x1, +y1]); };
  C.createRadialGradient = function (x0, y0, r0, x1, y1, r1) { return new CanvasGradient("radial", [+x0, +y0, +r0, +x1, +y1, +r1]); };
  C.createConicGradient = function (a, x, y) { return new CanvasGradient("conic", [+a, +x, +y]); };
  C.createPattern = function (src, rep) {
    if (!src) return null;
    var p = { _pattern: { src: imageSourceRef(src), rep: rep == null || rep === "" ? "repeat" : String(rep) } };
    p.setTransform = function () { warn("canvas: CanvasPattern.setTransform is not supported"); };
    return p;
  };
  C.drawImage = function (img) {
    if (!isElement(img)) return;
    this._ops.push(["drawImage", imageSourceRef(img)].concat([].slice.call(arguments, 1).map(Number)));
  };
  C.measureText = function (text) {
    var r = [0, 0, 0];
    try { r = JSON.parse(__blitz_native("measureText", JSON.stringify([this._s.font, String(text)]))); } catch (e) { /* no metrics */ }
    var w = r[0], asc = r[1], desc = r[2];
    var left = this._s.textAlign === "center" ? w / 2 : this._s.textAlign === "right" || this._s.textAlign === "end" ? w : 0;
    return { width: w, actualBoundingBoxLeft: left, actualBoundingBoxRight: w - left, actualBoundingBoxAscent: asc, actualBoundingBoxDescent: desc,
      fontBoundingBoxAscent: asc, fontBoundingBoxDescent: desc, emHeightAscent: asc, emHeightDescent: desc, alphabeticBaseline: 0, hangingBaseline: asc * 0.8, ideographicBaseline: -desc };
  };
  C.createImageData = function (w, h) { if (w && w.width) { h = w.height; w = w.width; } return { width: w, height: h, data: new Uint8ClampedArray(w * h * 4), colorSpace: "srgb" }; };
  // the source canvases these calls read, with the calls each received since its last flush
  function pendingSources(ops) {
    var out = {};
    for (var i = 0; i < ops.length; i++) {
      var op = ops[i], src = op[0] === "drawImage" ? op[1] :
        ((op[0] === "fillStyle" || op[0] === "strokeStyle") && op[1] && op[1].pattern ? op[1].pattern.src : null);
      var el = src && canvasSources[src.ref], cx = el && el._uvCtx;
      if (cx && cx instanceof Context2D && !out[src.ref]) out[src.ref] = { w: el.width, h: el.height, ops: cx._ops };
    }
    return out;
  }
  C.getImageData = function (x, y, w, h) {
    // the canvas's last flushed frame with this frame's calls so far replayed on top (Rust)
    var img = this.createImageData(w, h), b64 = "";
    try {
      b64 = __blitz_native("canvasPixels", JSON.stringify({ ref: elementRef(this.canvas).el, w: this.canvas.width, h: this.canvas.height, ops: this._ops, srcs: pendingSources(this._ops), x: +x || 0, y: +y || 0, rw: img.width, rh: img.height }));
    } catch (e) { warn("canvas: getImageData failed: " + e); return img; }
    var bytes = base64ToBytes(b64), d = img.data;
    for (var i = 0; i < d.length && i < bytes.length; i++) d[i] = bytes[i];
    return img;
  };
  C.putImageData = function (img, dx, dy) {
    this._ops.push(["putImageData", bufferToBase64(img.data), img.width, img.height, +dx || 0, +dy || 0]);
  };
  var elProto = Object.getPrototypeOf(document.documentElement);
  var isCanvas = function (el) { return String(el.tagName).toLowerCase() === "canvas"; };
  var baseGetContext = elProto.getContext;
  elProto.getContext = function (type) {
    if (!isCanvas(this)) return baseGetContext ? baseGetContext.apply(this, arguments) : null;
    if (type === "2d") return this._uvCtx || (this._uvCtx = new Context2D(this));
    if (/webgl/.test(String(type))) {
      if (this._uvCtx) return this._uvCtx instanceof GLContext ? this._uvCtx : null;
      if (__blitz_native("glAvailable") !== "true") { warn("canvas: WebGL needs ANGLE (not found)"); return null; }
      return (this._uvCtx = new GLContext(this, String(type) === "webgl2" ? 2 : 1, arguments[1]));
    }
    return null;
  };
  ["width", "height"].forEach(function (dim) {
    var def = dim === "width" ? 300 : 150;
    var d = Object.getOwnPropertyDescriptor(elProto, dim);
    Object.defineProperty(elProto, dim, {
      configurable: true,
      get: function () {
        if (isCanvas(this)) { var v = parseInt(this.getAttribute(dim), 10); return isNaN(v) ? def : v; }
        if (String(this.tagName).toLowerCase() === "img") {
          // Chrome: the attribute, else the rendered box, else the intrinsic size (three.js sizes textures by it)
          var a = parseInt(this.getAttribute(dim), 10);
          if (!isNaN(a)) return a;
          var box = this.parentNode && document.documentElement.contains(this) ? this[dim === "width" ? "clientWidth" : "clientHeight"] : 0;
          return box > 0 ? box : this[dim === "width" ? "naturalWidth" : "naturalHeight"] || 0;
        }
        return d && d.get ? d.get.call(this) : +(this.getAttribute(dim) || 0);
      },
      set: function (v) {
        if (d && d.set && !isCanvas(this)) { d.set.call(this, v); return; }
        this.setAttribute(dim, String(Math.max(0, Math.floor(+v) || 0)));
        if (isCanvas(this) && this._uvCtx && typeof this._uvCtx.reset === "function") this._uvCtx.reset();
      },
    });
  });
  // the canvas's pixels (last flushed frame plus this frame's calls so far) as a PNG data URL (Rust)
  elProto.toDataURL = function () {
    if (!isCanvas(this) || !this.width || !this.height) return "data:,";
    var ctx = this._uvCtx;
    if (ctx && !(ctx instanceof Context2D)) { warn("canvas: toDataURL of a WebGL canvas is not supported"); return "data:,"; }
    try {
      return __blitz_native("canvasPixels", JSON.stringify({ ref: elementRef(this).el, w: this.width, h: this.height,
        ops: ctx ? ctx._ops : [], srcs: ctx ? pendingSources(ctx._ops) : {}, x: 0, y: 0, rw: this.width, rh: this.height, png: true }));
    } catch (e) { warn("canvas: toDataURL failed: " + e); return "data:,"; }
  };
  elProto.toBlob = function (cb) { if (typeof cb === "function") setTimeout(function () { cb(null); }, 0); };
  // The composition format's seek event (canvas / three.js adapters): collected here, dispatched per frame by Rust
  var baseAdd = g.addEventListener, baseRemove = g.removeEventListener;
  g.addEventListener = function (type, fn) {
    if (type === "hf-seek" && typeof fn === "function") { seekListeners.push(fn); send({ type: "seek-listener" }); return; }
    return baseAdd ? baseAdd.apply(this, arguments) : undefined;
  };
  g.removeEventListener = function (type, fn) {
    if (type === "hf-seek") { var k = seekListeners.indexOf(fn); if (k >= 0) seekListeners.splice(k, 1); return; }
    return baseRemove ? baseRemove.apply(this, arguments) : undefined;
  };
  // The composition format's Lottie adapter: seek every registered animation to its composition's local time
  var compStarts = {};
  function lottieInstances() {
    var list = (g.__hfLottie || []).slice();
    try {
      var reg = g.lottie && typeof g.lottie.getRegisteredAnimations === "function" ? g.lottie.getRegisteredAnimations() : [];
      (reg || []).forEach(function (a) { if (list.indexOf(a) < 0) list.push(a); });
    } catch (e) { /* no registry */ }
    return list;
  }
  function seekLottie(pageTime) {
    var list = lottieInstances();
    for (var i = 0; i < list.length; i++) {
      var anim = list[i];
      try {
        var el = anim.wrapper || anim.container || (anim.renderer && anim.renderer.svgElement) || null;
        var comp = el && el.closest ? el.closest("[data-composition-id]") : null;
        var start = comp ? compStarts[comp.getAttribute("data-composition-id")] || 0 : 0;
        var time = Math.max(0, Math.max(0, pageTime) - start);
        if (typeof anim.goToAndStop === "function" && typeof anim.frameRate === "number") {
          var frame = time * anim.frameRate, total = anim.totalFrames;
          if (total > 0 && anim.loop === true) anim.goToAndStop(((frame % total) + total) % total, true);
          else if (total > 0 && frame >= total) anim.goToAndStop(total - 1, true);
          else anim.goToAndStop(time * 1000, false);
        } else if (typeof anim.setCurrentRawFrameValue === "function") {
          var fps = anim.frameRate || 30, tf = anim.totalFrames || 0, f = time * fps;
          if (tf > 0) anim.setCurrentRawFrameValue(Math.min(anim.loop ? ((f % tf) + tf) % tf : f, tf - 1));
        }
      } catch (e) { send({ type: "warn", message: "lottie seek failed: " + e }); }
    }
  }
  // ---- WebGL: a recording context; src/webgl.rs runs the calls on ANGLE (SwiftShader) ----
  var glContexts = [], glConstants = null, glNextHandle = 1;
  var GL_EXTENSIONS = {
    EXT_color_buffer_float: "GL_EXT_color_buffer_float", EXT_color_buffer_half_float: "GL_EXT_color_buffer_half_float",
    EXT_float_blend: "GL_EXT_float_blend", OES_texture_float_linear: "GL_OES_texture_float_linear",
    OES_texture_half_float_linear: "GL_OES_texture_half_float_linear", EXT_texture_filter_anisotropic: "GL_EXT_texture_filter_anisotropic",
    EXT_texture_norm16: "GL_EXT_texture_norm16", OES_draw_buffers_indexed: "GL_OES_draw_buffers_indexed",
    WEBGL_multi_draw: "GL_ANGLE_multi_draw", EXT_clip_control: "GL_EXT_clip_control", EXT_depth_clamp: "GL_EXT_depth_clamp",
    WEBGL_debug_renderer_info: null, KHR_parallel_shader_compile: null, WEBGL_lose_context: null,
  };
  var GL_EXTENSION_CONSTANTS = {
    EXT_texture_filter_anisotropic: { TEXTURE_MAX_ANISOTROPY_EXT: 0x84fe, MAX_TEXTURE_MAX_ANISOTROPY_EXT: 0x84ff },
    WEBGL_debug_renderer_info: { UNMASKED_VENDOR_WEBGL: 0x9245, UNMASKED_RENDERER_WEBGL: 0x9246 },
    KHR_parallel_shader_compile: { COMPLETION_STATUS_KHR: 0x91b1 },
    EXT_color_buffer_half_float: { RGBA16F_EXT: 0x881a, RGB16F_EXT: 0x881b, FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT: 0x8211, UNSIGNED_NORMALIZED_EXT: 0x8c17 },
  };
  function glEncode(v) {
    if (v === null || v === undefined) return null;
    var t = typeof v;
    if (t === "number" || t === "boolean" || t === "string") return v;
    if (v.__glh) return { h: v.__glh };
    if (v.__glu) return { u: v.__glu };
    if (ArrayBuffer.isView(v)) return { b64: bufferToBase64(new Uint8Array(v.buffer, v.byteOffset, v.byteLength)) };
    if (v instanceof ArrayBuffer) return { b64: bufferToBase64(new Uint8Array(v)) };
    if (isElement(v)) return imageSourceRef(v);
    if (v.data && v.width) return { b64: bufferToBase64(new Uint8Array(v.data.buffer, v.data.byteOffset, v.data.byteLength)), w: v.width, h: v.height };
    return v;
  }
  function glTyped(v, kind, offset, length) {
    var T = kind === "i" ? Int32Array : kind === "ui" ? Uint32Array : Float32Array;
    var a = ArrayBuffer.isView(v) ? v : new T(v);
    if (offset || length) a = a.subarray(offset || 0, length ? (offset || 0) + length : undefined);
    return { b64: bufferToBase64(new Uint8Array(a.buffer, a.byteOffset, a.byteLength)) };
  }
  function GLContext(canvas, version, attrs) {
    this.canvas = canvas; this._v = version; this._ops = []; this._exts = {}; this._supported = null;
    this._attrs = Object.assign({ alpha: true, depth: true, stencil: false, antialias: true, premultipliedAlpha: true, preserveDrawingBuffer: false, powerPreference: "default", failIfMajorPerformanceCaveat: false, desynchronized: false }, attrs || {});
    if (!glConstants) {
      glConstants = JSON.parse(__blitz_native("glConstants"));
      for (var k in glConstants) GLContext.prototype[k] = glConstants[k];
    }
    glContexts.push(this);
  }
  var G = GLContext.prototype;
  Object.defineProperty(G, "drawingBufferWidth", { get: function () { return this.canvas.width; } });
  Object.defineProperty(G, "drawingBufferHeight", { get: function () { return this.canvas.height; } });
  G.drawingBufferColorSpace = "srgb";
  // 2D canvases the recorded GL calls upload (texImage2D(..., canvas)), with their pending calls
  function glPendingSources(ops) {
    var out = {};
    for (var i = 0; i < ops.length; i++) {
      var op = ops[i];
      for (var j = 1; j < op.length; j++) {
        var a = op[j], el = a && typeof a === "object" && a.ref != null && canvasSources[a.ref], cx = el && el._uvCtx;
        if (cx && cx instanceof Context2D && !out[a.ref]) out[a.ref] = { w: el.width, h: el.height, ops: cx._ops };
      }
    }
    return out;
  }
  G._sync = function (q) {
    var payload = JSON.stringify({ id: elementRef(this.canvas).el, w: this.canvas.width, h: this.canvas.height, v: this._v, attrs: this._attrs, ops: this._ops, srcs: glPendingSources(this._ops), q: q });
    this._ops = [];
    var r;
    try { r = __blitz_native("gl", payload); } catch (e) { throw new Error("WebGL: " + e.message); }
    return JSON.parse(r);
  };
  G.getContextAttributes = function () { return Object.assign({}, this._attrs); };
  G.isContextLost = function () { return false; };
  G.getError = function () { return 0; };
  G.flush = G.finish = function () {};
  G.hint = function () {};
  ["buffer", "texture", "framebuffer", "renderbuffer", "program", "vertexArray", "sampler", "query", "transformFeedback"].forEach(function (kind) {
    var cap = kind.charAt(0).toUpperCase() + kind.slice(1);
    G["create" + cap] = function () { var id = glNextHandle++; this._ops.push(["create", kind, id]); return { __glh: id, __kind: kind }; };
    G["delete" + cap] = function (h) { if (h && h.__glh) this._ops.push(["delete", { h: h.__glh }]); };
    G["is" + cap] = function (h) { return !!(h && h.__glh && h.__kind === kind); };
  });
  G.createShader = function (type) { var id = glNextHandle++; this._ops.push(["create", "shader:" + type, id]); return { __glh: id, __kind: "shader" }; };
  G.deleteShader = function (h) { if (h && h.__glh) this._ops.push(["delete", { h: h.__glh }]); };
  G.isShader = function (h) { return !!(h && h.__kind === "shader"); };
  G.createVertexArrayOES = G.createVertexArray;
  var GL_RECORDED = ["shaderSource", "compileShader", "attachShader", "detachShader", "linkProgram", "bindAttribLocation", "useProgram",
    "bindBuffer", "bindBufferBase", "bindBufferRange", "vertexAttribPointer", "vertexAttribIPointer", "enableVertexAttribArray",
    "disableVertexAttribArray", "vertexAttribDivisor", "vertexAttrib1f", "vertexAttrib2f", "vertexAttrib3f", "vertexAttrib4f",
    "bindVertexArray", "activeTexture", "bindTexture", "texParameteri", "texParameterf", "pixelStorei", "texStorage2D", "texStorage3D",
    "generateMipmap", "copyTexSubImage2D", "bindFramebuffer", "bindRenderbuffer", "framebufferTexture2D", "framebufferTextureLayer",
    "framebufferRenderbuffer", "renderbufferStorage", "renderbufferStorageMultisample", "blitFramebuffer", "readBuffer", "enable",
    "disable", "blendFunc", "blendFuncSeparate", "blendEquation", "blendEquationSeparate", "blendColor", "depthFunc", "depthMask",
    "depthRange", "colorMask", "cullFace", "frontFace", "polygonOffset", "scissor", "viewport", "lineWidth", "sampleCoverage",
    "stencilFunc", "stencilFuncSeparate", "stencilMask", "stencilMaskSeparate", "stencilOp", "stencilOpSeparate", "clearColor",
    "clearDepth", "clearStencil", "clear", "clearBufferfi", "drawArrays", "drawElements", "drawArraysInstanced", "drawElementsInstanced",
    "drawRangeElements", "uniformBlockBinding", "bindSampler", "samplerParameteri", "samplerParameterf",
    "uniform1f", "uniform2f", "uniform3f", "uniform4f", "uniform1i", "uniform2i", "uniform3i", "uniform4i", "uniform1ui", "uniform2ui", "uniform3ui", "uniform4ui"];
  GL_RECORDED.forEach(function (n) {
    G[n] = function () { var a = [n]; for (var i = 0; i < arguments.length; i++) a.push(glEncode(arguments[i])); this._ops.push(a); };
  });
  G.bindVertexArrayOES = G.bindVertexArray;
  G.drawArraysInstancedANGLE = G.drawArraysInstanced; G.drawElementsInstancedANGLE = G.drawElementsInstanced; G.vertexAttribDivisorANGLE = G.vertexAttribDivisor;
  G.drawBuffers = function (list) { this._ops.push(["drawBuffers", [].slice.call(list)]); };
  G.drawBuffersWEBGL = G.drawBuffers;
  G.invalidateFramebuffer = function (t, list) { this._ops.push(["invalidateFramebuffer", t, [].slice.call(list)]); };
  G.invalidateSubFramebuffer = function () {};
  [["fv", "f"], ["iv", "i"], ["uiv", "ui"]].forEach(function (p) {
    [1, 2, 3, 4].forEach(function (n) {
      var name = "uniform" + n + p[0];
      G[name] = function (loc, v, off, len) { this._ops.push([name, glEncode(loc), glTyped(v, p[1], off, len)]); };
    });
  });
  [2, 3, 4].forEach(function (n) {
    var name = "uniformMatrix" + n + "fv";
    G[name] = function (loc, tr, v, off, len) { this._ops.push([name, glEncode(loc), !!tr, glTyped(v, "f", off, len)]); };
  });
  ["clearBufferfv", "clearBufferiv", "clearBufferuiv"].forEach(function (name) {
    var k = name === "clearBufferfv" ? "f" : name === "clearBufferiv" ? "i" : "ui";
    G[name] = function (buf, i, v, off) { this._ops.push([name, buf, i, glTyped(v, k, off)]); };
  });
  G.bufferData = function (target, data, usage, srcOffset, length) {
    if (typeof data === "number") { this._ops.push(["bufferData", target, data, usage]); return; }
    if (ArrayBuffer.isView(data) && (srcOffset || length)) {
      var bpe = data.BYTES_PER_ELEMENT || 1, start = (srcOffset || 0) * bpe;
      data = new Uint8Array(data.buffer, data.byteOffset + start, length ? length * bpe : data.byteLength - start);
    }
    this._ops.push(["bufferData", target, glEncode(data), usage]);
  };
  G.bufferSubData = function (target, offset, data, srcOffset, length) {
    if (ArrayBuffer.isView(data) && (srcOffset || length)) {
      var bpe = data.BYTES_PER_ELEMENT || 1, start = (srcOffset || 0) * bpe;
      data = new Uint8Array(data.buffer, data.byteOffset + start, length ? length * bpe : data.byteLength - start);
    }
    this._ops.push(["bufferSubData", target, offset, glEncode(data)]);
  };
  G.texImage2D = function () {
    var a = arguments;
    if (a.length === 6) { this._ops.push(["texImage2D", a[0], a[1], a[2], a[3], a[4], glEncode(a[5])]); return; }
    if (a.length >= 9 && isElement(a[8])) { this._ops.push(["texImage2D", a[0], a[1], a[2], a[6], a[7], glEncode(a[8])]); return; }
    var data = a[8];
    if (ArrayBuffer.isView(data) && a[9]) data = data.subarray(a[9]);
    this._ops.push(["texImage2D", a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], typeof data === "number" ? null : glEncode(data)]);
  };
  G.texSubImage2D = function () {
    var a = arguments;
    if (a.length === 7) { this._ops.push(["texSubImage2D", a[0], a[1], a[2], a[3], a[4], a[5], glEncode(a[6])]); return; }
    if (a.length >= 9 && isElement(a[8])) { this._ops.push(["texSubImage2D", a[0], a[1], a[2], a[3], a[6], a[7], glEncode(a[8])]); return; }
    var data = a[8];
    if (ArrayBuffer.isView(data) && a[9]) data = data.subarray(a[9]);
    this._ops.push(["texSubImage2D", a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], glEncode(data)]);
  };
  G.texImage3D = function () {
    var a = [].slice.call(arguments);
    this._ops.push(["texImage3D"].concat(a.slice(0, 10)).concat([ArrayBuffer.isView(a[10]) ? glEncode(a[10]) : null]));
  };
  G.compressedTexImage2D = G.compressedTexSubImage2D = function () { warn("WebGL: compressed textures are not supported"); };
  // synchronous answers (they run everything recorded so far first)
  G.getShaderParameter = function (sh, p) { return this._sync(["getShaderParameter", glEncode(sh), p]); };
  G.getShaderInfoLog = function (sh) { return this._sync(["getShaderInfoLog", glEncode(sh)]) || ""; };
  G.getShaderSource = function () { return null; };
  G.getProgramParameter = function (pr, p) {
    if (p === 0x91b1) return true; // COMPLETION_STATUS_KHR: compiled synchronously
    return this._sync(["getProgramParameter", glEncode(pr), p]);
  };
  G.getProgramInfoLog = function (pr) { return this._sync(["getProgramInfoLog", glEncode(pr)]) || ""; };
  G.getActiveUniform = function (pr, i) { return this._sync(["getActiveUniform", glEncode(pr), i]); };
  G.getActiveAttrib = function (pr, i) { return this._sync(["getActiveAttrib", glEncode(pr), i]); };
  G.getUniformLocation = function (pr, name) { var id = this._sync(["getUniformLocation", glEncode(pr), String(name)]); return id == null ? null : { __glu: id }; };
  G.getAttribLocation = function (pr, name) { return this._sync(["getAttribLocation", glEncode(pr), String(name)]); };
  G.getFragDataLocation = function (pr, name) { return this._sync(["getFragDataLocation", glEncode(pr), String(name)]); };
  G.getUniformBlockIndex = function (pr, name) { return this._sync(["getUniformBlockIndex", glEncode(pr), String(name)]); };
  G.getActiveUniformBlockParameter = function (pr, i, p) { return this._sync(["getActiveUniformBlockParameter", glEncode(pr), i, p]); };
  G.getActiveUniformBlockName = function (pr, i) { return this._sync(["getActiveUniformBlockName", glEncode(pr), i]); };
  G.checkFramebufferStatus = function (t) { return this._sync(["checkFramebufferStatus", t]); };
  G.getShaderPrecisionFormat = function (s, p) { return this._sync(["getShaderPrecisionFormat", s, p]); };
  G.getParameter = function (p) {
    if (p === 0x9245) return "Google Inc. (Google)";
    if (p === 0x9246) return this._sync(["getParameter", 0x1f01]);
    var v = this._sync(["getParameter", p]);
    if (p === 0x0ba2 || p === 0x0c10) return new Int32Array(v);
    if (p === 0x0c22 || p === 0x8005 || p === 0x846e || p === 0x846d || p === 0x0b70) return new Float32Array(v);
    if (p === 0x1f02 && this._v === 2) return "WebGL 2.0 (OpenGL ES 3.0 Chromium)";
    if (p === 0x1f02) return "WebGL 1.0 (OpenGL ES 2.0 Chromium)";
    if (p === 0x8b8c) return this._v === 2 ? "WebGL GLSL ES 3.00 (OpenGL ES GLSL ES 3.0 Chromium)" : "WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)";
    if (p === 0x1f00) return "WebKit";
    if (p === 0x1f01) return "WebKit WebGL";
    return v;
  };
  G.readPixels = function (x, y, w, h, format, type, out, off) {
    if (!ArrayBuffer.isView(out)) return;
    var bytes = b64ToBytes(this._sync(["readPixels", x, y, w, h]));
    var dst = new Uint8Array(out.buffer, out.byteOffset + (off || 0) * (out.BYTES_PER_ELEMENT || 1));
    dst.set(bytes.subarray(0, Math.min(bytes.length, dst.length)));
  };
  G.getSupportedExtensions = function () {
    if (!this._supported) {
      var gl = this._sync(["getSupportedExtensions"]) || [];
      var requestable = String(this._sync(["getParameter", 0x93a8]) || "").split(" ");
      var have = {};
      gl.concat(requestable).forEach(function (e) { have[e] = 1; });
      this._supported = Object.keys(GL_EXTENSIONS).filter(function (n) { return GL_EXTENSIONS[n] === null || have[GL_EXTENSIONS[n]]; });
    }
    return this._supported.slice();
  };
  G.getExtension = function (name) {
    if (this._exts[name]) return this._exts[name];
    if (this.getSupportedExtensions().indexOf(name) < 0) return null;
    if (GL_EXTENSIONS[name]) this._ops.push(["requestExtension", GL_EXTENSIONS[name]]);
    var ext = Object.assign({}, GL_EXTENSION_CONSTANTS[name] || {});
    if (name === "WEBGL_lose_context") { ext.loseContext = function () {}; ext.restoreContext = function () {}; }
    this._exts[name] = ext;
    return ext;
  };
  G.fenceSync = function () { return { __sync: true }; };
  G.clientWaitSync = function () { return 0x911a; }; // ALREADY_SIGNALED
  G.waitSync = G.deleteSync = function () {};
  G.getSyncParameter = function () { return 0x9119; }; // SIGNALED
  G.beginQuery = G.endQuery = function () {};
  G.getQueryParameter = function () { return 0; };
  g.WebGLRenderingContext = g.WebGL2RenderingContext = GLContext;
  function flushGL() {
    glContexts.forEach(function (ctx) {
      if (!ctx.canvas) return;
      send({ type: "webgl", ref: elementRef(ctx.canvas).el, w: ctx.canvas.width, h: ctx.canvas.height, v: ctx._v, attrs: ctx._attrs, ops: ctx._ops });
      ctx._ops = [];
    });
  }
  function flushCanvases() {
    if (glContexts.length) flushGL();
    canvasContexts.forEach(function (ctx) {
      if (!ctx._ops.length || !ctx.canvas) return;
      send({ type: "canvas", ref: elementRef(ctx.canvas).el, w: ctx.canvas.width, h: ctx.canvas.height, ops: ctx._ops });
      ctx._ops = [];
    });
  }

  // ---- FontFace / document.fonts.add: an @font-face rule Blitz loads like any other ----------
  function base64ToBytes(s) {
    var chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/", lut = {};
    for (var k = 0; k < 64; k++) lut[chars[k]] = k;
    s = String(s || "").replace(/[^A-Za-z0-9+/]/g, "");
    var out = new Uint8Array(Math.floor(s.length * 3 / 4)), o = 0;
    for (var i = 0; i + 1 < s.length; i += 4) {
      var a = lut[s[i]], b = lut[s[i + 1]], c = lut[s[i + 2]], d = lut[s[i + 3]];
      out[o++] = (a << 2) | (b >> 4);
      if (c !== undefined) out[o++] = ((b & 15) << 4) | (c >> 2);
      if (d !== undefined) out[o++] = ((c & 3) << 6) | d;
    }
    return out.subarray(0, o);
  }
  function bufferToBase64(buf) {
    // a view keeps its own window into the buffer (GLTF accessors share one bufferView)
    var bytes = ArrayBuffer.isView(buf) ? new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength) : new Uint8Array(buf.buffer || buf);
    var chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/", parts = [], line = "";
    for (var i = 0; i < bytes.length; i += 3) {
      var a = bytes[i], b = bytes[i + 1], c = bytes[i + 2];
      line += chars[a >> 2] + chars[((a & 3) << 4) | ((b || 0) >> 4)] + (b === undefined ? "=" : chars[((b & 15) << 2) | ((c || 0) >> 6)]) + (c === undefined ? "=" : chars[c & 63]);
      if (line.length >= 8192) { parts.push(line); line = ""; }
    }
    parts.push(line);
    return parts.join("");
  }
  if (typeof g.FontFace !== "function") {
    g.FontFace = function (family, source, descriptors) {
      var d = descriptors || {};
      this.family = String(family).replace(/^["']|["']$/g, "");
      this.source = source;
      this.weight = d.weight || "normal";
      this.style = d.style || "normal";
      this.stretch = d.stretch || "normal";
      this.unicodeRange = d.unicodeRange || "U+0-10FFFF";
      this.display = d.display || "auto";
      this.status = "unloaded";
      this.loaded = Promise.resolve(this);
    };
    g.FontFace.prototype.load = function () { this.status = "loaded"; return Promise.resolve(this); };
  }
  // document.fonts.ready / load(): Chrome resolves them once the page's pending faces have loaded, so code
  // waiting on them measures text in the web fonts (the renderer lays text out in fallbacks while they load)
  if (document.fonts) {
    (function (fs) {
      var loaded = function (v) {
        try { __blitz_native("fontsLoaded", ""); } catch (e) { /* no renderer */ }
        return v;
      };
      var ready = null;
      Object.defineProperty(fs, "ready", { configurable: true, get: function () { return ready || (ready = Promise.resolve(fs).then(loaded)); } });
      var load = fs.load;
      fs.load = function () { return Promise.resolve(load.apply(fs, arguments)).then(loaded); };
    })(document.fonts);
  }
  if (document.fonts) {
    var addedFaces = [];
    document.fonts.add = function (face) {
      if (!face || addedFaces.indexOf(face) !== -1) return document.fonts;
      addedFaces.push(face);
      var src = face.source;
      if (src && typeof src !== "string") src = "url(data:font/woff2;base64," + bufferToBase64(src) + ")";
      var css = "@font-face{font-family:\"" + face.family + "\";src:" + src + ";font-weight:" + face.weight + ";font-style:" + face.style +
        ";font-stretch:" + face.stretch + ";unicode-range:" + face.unicodeRange + ";font-display:block}";
      var style = document.createElement("style");
      style.setAttribute("data-uv-fontface", face.family);
      style.textContent = css;
      (document.head || document.documentElement).appendChild(style);
      document.fonts.size = addedFaces.length;
      return document.fonts;
    };
    document.fonts.delete = function () { return false; };
    document.fonts.has = function (face) { return addedFaces.indexOf(face) !== -1; };
  }

  // ---- the HTML composition format runtime helpers compositions call --------------------------------------
  function slugify(name) {
    var s = String(name).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
    return s || "node";
  }
  function declarers() {
    var out = [];
    if (document.documentElement && document.documentElement.hasAttribute("data-composition-variables")) out.push(document.documentElement);
    var list = document.querySelectorAll("[data-composition-variables]");
    for (var i = 0; i < list.length; i++) if (out.indexOf(list[i]) < 0) out.push(list[i]);
    return out;
  }
  function declaredDefaults(el) {
    var out = {};
    var raw;
    try { raw = JSON.parse(el.getAttribute("data-composition-variables") || "[]"); } catch (e) { return out; }
    if (!Array.isArray(raw)) return out;
    raw.forEach(function (entry) { if (entry && typeof entry.id === "string" && "default" in entry) out[entry.id] = entry.default; });
    return out;
  }
  function renderOverrides() {
    var v = g.__hfVariables;
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  }
  function getVariables() {
    var merged = {};
    declarers().forEach(function (el) { Object.assign(merged, declaredDefaults(el)); });
    return Object.assign(merged, renderOverrides());
  }
  function injectCssVariables() {
    var overrides = renderOverrides();
    declarers().forEach(function (el) {
      var defaults = declaredDefaults(el);
      var ids = Object.keys(defaults).concat(Object.keys(overrides).filter(function (k) { return !(k in defaults); }));
      ids.forEach(function (id) {
        var value = id in overrides ? overrides[id] : defaults[id];
        if (value == null || typeof value === "object" || value === "") return;
        var name = "--" + slugify(id);
        if (!(id in overrides) && el.style.getPropertyValue(name)) return;
        el.style.setProperty(name, String(value));
      });
    });
  }
  g.__raea = {
    getVariables: getVariables,
    assetUrl: function (p) { return p; },
    fitTextFontSize: function (el, opts) { return opts && opts.maxSize ? opts.maxSize : undefined; },
    registerRuntimeDataHandler: function () {},
    setRuntimeData: function () {},
    clearRuntimeData: function () {},
    registerFrameSource: function () { warn("registerFrameSource (custom frame sources) is not supported by the blitz renderer"); },
    createFilmBridge: function () { return {}; },
  };
  try { injectCssVariables(); } catch (e) {}
  // The composition format's applyVariableBindings (runtime init, after the composition's scripts)
  function scopedVariables(el) {
    var scope = el;
    while (scope && !(scope.nodeType === 1 && scope.hasAttribute("data-composition-id"))) scope = scope.parentNode;
    var id = scope ? String(scope.getAttribute("data-composition-id") || "").trim() : "";
    var byComp = g.__hfVariablesByComp;
    if (id && byComp && byComp[id]) return byComp[id];
    var v = getVariables();
    return v && typeof v === "object" ? v : renderOverrides();
  }
  function cssVariableValue(v) {
    if (typeof v === "string" || typeof v === "number" || typeof v === "boolean") return String(v);
    if (v && typeof v === "object" && typeof v.name === "string" && v.name) return v.name;
    return null;
  }
  function safeMediaUrl(url) {
    var s = url.replace(/[\u0000-\u0020]/g, ""), m = /^([a-z][a-z0-9+.-]*):/i.exec(s);
    if (!m) return true;
    var p = m[1].toLowerCase();
    return p === "https" || p === "http" || p === "blob" || (p === "data" && /^data:image\//i.test(s));
  }
  g.__uvVariableBindings = function () {
    var body = document.body;
    var top = document.querySelector("[data-hf-root]") || document.getElementById("stage") || (body && body.firstElementChild) || body;
    var roots = [], list = document.querySelectorAll("[data-composition-id]"), i;
    if (top) roots.push(top);
    for (i = 0; i < list.length; i++) if (roots.indexOf(list[i]) < 0) roots.push(list[i]);
    roots.forEach(function (root) {
      if (!root.style || String(root.namespaceURI || "").indexOf("svg") >= 0) return;
      var values = scopedVariables(root);
      Object.keys(values).forEach(function (id) {
        var css = cssVariableValue(values[id]);
        if (css !== null) root.style.setProperty("--" + id, css.replace(/[;{}<>\r\n]/g, ""));
      });
    });
    var media = document.querySelectorAll("[data-var-src]");
    for (i = 0; i < media.length; i++) {
      var el = media[i], vid = String(el.getAttribute("data-var-src") || "").trim();
      if (!vid || ["img", "video", "audio", "source"].indexOf(String(el.tagName).toLowerCase()) < 0) continue;
      var val = scopedVariables(el)[vid];
      var url = typeof val === "string" ? val : val && typeof val === "object" && typeof val.url === "string" ? val.url : "";
      if (url && safeMediaUrl(url) && el.getAttribute("src") !== url) el.setAttribute("src", url);
    }
    var texts = document.querySelectorAll("[data-var-text]");
    for (i = 0; i < texts.length; i++) {
      var t = texts[i], tid = String(t.getAttribute("data-var-text") || "").trim();
      var tv = tid ? scopedVariables(t)[tid] : undefined;
      if (typeof tv !== "string" && typeof tv !== "number" && typeof tv !== "boolean") continue;
      if (!t.firstElementChild) { t.textContent = String(tv); continue; }
      var written = false;
      Array.prototype.slice.call(t.childNodes).forEach(function (n) {
        if (n.nodeType !== 3) return;
        n.nodeValue = written ? "" : String(tv);
        written = true;
      });
      if (!written) t.insertBefore(document.createTextNode(String(tv)), t.firstChild);
    }
  };
  if (document.readyState === "loading" && typeof document.addEventListener === "function") {
    document.addEventListener("DOMContentLoaded", function () { try { injectCssVariables(); } catch (e) {} });
  }

  // ---- renderer bridge -------------------------------------------------------------------
  function serializeCustomEases(node) {
    if (node.kind === "tween") {
      if (typeof node.ease === "string" && customEases[node.ease]) node.ease = customEases[node.ease];
      (node.items || []).forEach(function (it) {
        (it.keyframes || []).forEach(function (k) {
          if (typeof k.ease === "string" && customEases[k.ease]) k.ease = customEases[k.ease];
          else if (typeof k.ease === "function" && k.ease.__uvEase) k.ease = k.ease.__uvEase;
        });
      });
    }
    (node.children || []).forEach(serializeCustomEases);
    return node;
  }

  // registered timelines report the composition time (the HTML composition format drives them with totalTime)
  function syncRegistered(t) {
    var regs = g.__timelines || {};
    for (var rk in regs) {
      var rt = regs[rk];
      if (rt && rt._dur && animsById[rt._id] === rt) {
        var rd = rt.totalDuration ? rt.totalDuration() : rt._dur();
        var local = Math.max(0, Math.min(t - ((compStarts && compStarts[rk]) || 0), rd || 0));
        rt._uvT = rt._uvTT = local;
        rt._uvP = rd > 0 ? local / rd : 1;
      }
    }
  }

  g.__hfb = {
    // Called by Rust after every script ran: the timeline registry and loose global tweens.
    // Per frame (Rust): the composition format's hf-seek event, then the canvases' recorded calls.
    seek: function (t) {
      seekLottie(t);
      g.__hfThreeTime = t;
      syncRegistered(t);
      var ev = { type: "hf-seek", detail: { time: t, waitUntil: function () {} } };
      seekListeners.slice().forEach(function (fn) {
        try { fn.call(g, ev); } catch (e) { send({ type: "warn", message: "hf-seek listener threw: " + e }); }
      });
      flushCanvases();
    },
    flushCanvases: function () { flushCanvases(); },
    compositionStarts: function (starts) { compStarts = starts || {}; },
    flush: function () {
      flushCanvases();
      if (lottieInstances().length) send({ type: "seek-listener" });
      live = true;
      var named = {};
      var tls = g.__timelines || {};
      for (var k in tls) if (tls[k] instanceof Animation) named[k] = tls[k]._id;
      var tree = serializeCustomEases(globalTimeline._serialize());
      send({ type: "timelines", root: tree, named: named });
    },
    // Called by Rust per frame for plain-object targets and callbacks.
    frame: function (objs, cbs, els, t) {
      // GSAP has the root's time set before its children's callbacks run
      if (t !== undefined) syncRegistered(t);
      for (var e = 0; els && e < els.length; e++) {
        var el = refElement(els[e][0]);
        if (!el) continue;
        var ev = els[e][1];
        for (var ek in ev) {
          if (ek === "scrollTop" || ek === "scrollLeft") continue;
          el[ek] = ev[ek];
        }
      }
      for (var i = 0; i < objs.length; i++) {
        var o = objects[objs[i][0]];
        if (!o) continue;
        var vals = objs[i][1];
        for (var k in vals) {
          if (typeof o[k] === "function") o[k](vals[k]);
          else o[k] = vals[k];
        }
      }
      for (var j = 0; j < cbs.length; j++) {
        // [callback, animation, time, progress, totalTime]
        var cb = cbs[j], c = callbacks[Array.isArray(cb) ? cb[0] : cb];
        if (!c) continue;
        var anim = Array.isArray(cb) ? animsById[cb[1]] : null;
        if (anim) { anim._uvT = +cb[2] || 0; anim._uvP = +cb[3] || 0; anim._uvTT = +cb[4] || 0; }
        try { c.fn.apply(c.scope || anim || null, c.params || []); } catch (e) { send({ type: "warn", message: "callback threw: " + e }); }
      }
    },
    scope: function (compId) {
      var root = null;
      var all = document.querySelectorAll("[data-composition-id]");
      for (var i = 0; i < all.length; i++) if (all[i].getAttribute("data-composition-id") === compId) { root = all[i]; break; }
      if (!root) return { document: document, gsap: gsap };
      var scopedQueryAll = function (sel) {
        var found = scopedAll(root, sel);
        return found.length ? found : Array.prototype.slice.call(document.querySelectorAll(sel));
      };
      var scopedDoc;
      try {
        scopedDoc = new Proxy(document, {
          get: function (t, p) {
            if (p === "querySelector") return function (sel) { return scopedQueryAll(sel)[0] || null; };
            if (p === "querySelectorAll") return function (sel) { return scopedQueryAll(sel); };
            if (p === "getElementsByClassName") return function (c) { return scopedQueryAll("." + String(c).trim().split(/\s+/).join(".")); };
            // The composition format's __hfGetElementById (compositionScoping.ts): a global match inside this
            // composition, else its root by authored id, else a scoped [data-hf-authored-id] / [id] match
            if (p === "getElementById") return function (id) {
              var found = document.getElementById(id);
              for (var a = found; a; a = a.parentNode) if (a === root) return found;
              var v = String(id), esc = v.replace(/["\\]/g, "\\$&");
              if (root.getAttribute("data-hf-authored-id") === v || root.id === v) return root;
              return root.querySelector('[data-hf-authored-id="' + esc + '"]') || root.querySelector('[id="' + esc + '"]') || null;
            };
            // The HTML composition format runs a mounted composition's scripts inside its root, so
            // `document.currentScript.closest(...)` finds that root
            if (p === "currentScript") return root;
            var v = t[p];
            return typeof v === "function" ? v.bind(t) : v;
          },
        });
      } catch (e) {
        scopedDoc = document;
      }
      var scoped = Object.create(gsap);
      ["to", "from", "fromTo", "set", "timeline", "delayedCall", "getProperty", "quickSetter", "quickTo"].forEach(function (k) {
        scoped[k] = function () { var a = arguments; return withScope(root, function () { return gsap[k].apply(gsap, a); }); };
      });
      scoped.utils = Object.create(gsap.utils);
      scoped.utils.toArray = function (t) { return toArray(t, root); };
      return { document: scopedDoc, gsap: scoped, root: root };
    },
    // Inline SVG is rasterized from attributes only, so styles that stylesheets put on SVG
    // children are copied onto them as presentation attributes (once, after setup).
    syncSvgStyles: function () {
      var INHERITED = ["fill", "stroke", "stroke-width", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-opacity", "fill-opacity", "visibility", "font-family", "font-size", "font-weight"];
      var PAINT = { fill: 1, stroke: 1, "stop-color": 1, "flood-color": 1, "lighting-color": 1 };
      // stylo serializes mixed / relative colours as CSS Color 4 color(srgb r g b [/ a]); usvg reads rgb()
      var svgColor = function (v) {
        return String(v).replace(/color\(srgb\s+(-?[\d.e+-]+)\s+(-?[\d.e+-]+)\s+(-?[\d.e+-]+)(?:\s*\/\s*([\d.e+-]+%?))?\s*\)/g, function (_, r, gg, b, a) {
          var c = function (x) { return Math.round(Math.max(0, Math.min(1, +x)) * 255); };
          var al = a === undefined ? 1 : /%$/.test(a) ? parseFloat(a) / 100 : +a;
          return al >= 1 ? "rgb(" + c(r) + "," + c(gg) + "," + c(b) + ")" : "rgba(" + c(r) + "," + c(gg) + "," + c(b) + "," + al + ")";
        });
      };
      var unreadable = /var\(|color-mix\(|color\(|oklch\(|oklab\(|lch\(|lab\(|light-dark\(|from /i;
      var paintValue = function (v, cs) {
        return /currentcolor/i.test(v) ? v.replace(/currentcolor/gi, svgColor(cs.getPropertyValue("color"))) : svgColor(v);
      };
      // each root <svg>'s own computed paint: the markup it's rasterized from carries only attributes
      var ROOT = ["fill", "stroke", "stroke-width", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-opacity", "fill-opacity"];
      var roots = document.querySelectorAll("svg");
      for (var r = 0; r < roots.length; r++) {
        var sv = roots[r], up = sv.parentNode;
        if (up && up.namespaceURI && String(up.namespaceURI).indexOf("svg") >= 0) continue;
        var rcs;
        try { rcs = getComputedStyle(sv); } catch (e) { continue; }
        for (var q = 0; q < ROOT.length; q++) {
          // presentation attributes aren't in the cascade here, so an authored one stays (as for children)
          if (sv.hasAttribute(ROOT[q])) continue;
          var rv = rcs.getPropertyValue(ROOT[q]);
          if (rv) sv.setAttribute(ROOT[q], PAINT[ROOT[q]] ? paintValue(rv, rcs) : rv);
        }
      }
      var list = document.querySelectorAll("svg *");
      for (var i = 0; i < list.length; i++) {
        var el = list[i];
        if (!el.namespaceURI || el.namespaceURI.indexOf("svg") < 0) continue;
        var cs, pcs;
        try { cs = getComputedStyle(el); pcs = el.parentElement ? getComputedStyle(el.parentElement) : null; } catch (e) { continue; }
        // inline paint declarations usvg can't parse take their computed value
        if (el.style) {
          for (var pp in PAINT) {
            var iv = el.style.getPropertyValue(pp);
            if (iv && unreadable.test(iv)) el.style.setProperty(pp, svgColor(cs.getPropertyValue(pp)));
          }
        }
        for (var j = 0; j < INHERITED.length; j++) {
          var p = INHERITED[j];
          if (el.hasAttribute(p)) {
            var av = el.getAttribute(p);
            if (PAINT[p] && unreadable.test(av)) el.setAttribute(p, svgColor(cs.getPropertyValue(p)));
            continue;
          }
          var v = cs.getPropertyValue(p);
          if (!v) continue;
          var pv = pcs ? pcs.getPropertyValue(p) : "";
          if (v !== pv) el.setAttribute(p, PAINT[p] ? paintValue(v, cs) : v);
        }
        var op = cs.getPropertyValue("opacity");
        if (op && op !== "1" && !el.hasAttribute("opacity")) el.setAttribute("opacity", op);
        if (cs.getPropertyValue("display") === "none" && !el.hasAttribute("display")) el.setAttribute("display", "none");
      }
    },
    objectValue: function (ref, prop) {
      var o = objects[ref];
      return o ? o[prop] : undefined;
    },
  };
})();
