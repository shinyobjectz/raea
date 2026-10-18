"""Writes the render path's K kernels: composite.k (up to four NV12 clips over the camera's target, then the
premultiplied overlay) and nv12buf.k (nv12.k reading the composite buffer F instead of the target texture).
python3 kernels/gen.py"""
SLOTS = 4
L = ["/ The frame on the GPU: the camera's target T is the background (clear colour, 3D), then up to four clips from",
     "/ their NV12 planes (P0..P3) by z, then the Blitz overlay O (premultiplied RGBA). Clip slot s's parameters are",
     "/ words s*12.. of C: on, box x0 y0 x1 y1, source w h, crop origin x y and step x y (16.16), opacity (0..256).",
     "/ A slot with on = 2 is masked: byte s of M's word for the pixel is its coverage (0..255), scaling its opacity.",
     "/ Luma and chroma are bilinear; YUV is BT.709 limited range in 8.8 fixed point. One RGBA word per pixel.",
     "x:i!w", "y:i%w",
     "bg:tex[T;x;y]",
     "r:and[bg;255]", "g:and[shr[bg;8];255]", "b:and[shr[bg;16];255]"]
for s in range(SLOTS):
    P, c = f"P{s}", lambda k: f"word[C;{s * 12 + k}]"
    L += [f"/ clip slot {s}",
          f"on{s}:{c(0)}", f"bx{s}:{c(1)}", f"by{s}:{c(2)}", f"ex{s}:{c(3)}", f"ey{s}:{c(4)}",
          f"sw{s}:{c(5)}", f"sh{s}:{c(6)}", f"cx{s}:{c(7)}", f"cy{s}:{c(8)}", f"dx{s}:{c(9)}", f"dy{s}:{c(10)}", f"al{s}:{c(11)}",
          f"in{s}:on{s}*(x>bx{s}-1)*(x<ex{s})*(y>by{s}-1)*(y<ey{s})",
          # a masked slot (on = 2) takes its coverage from byte s of M; unmasked, 255 (and am = al exactly)
          f"ms{s}:on{s}>1",
          f"cov{s}:(ms{s}*and[shr[word[M;i];{8 * s}];255])+((1-ms{s})*255)",
          f"am{s}:((al{s}*cov{s})+127)%255",
          # only pixels the clip covers do its work
          f"when in{s}",
          # the sample centre in 16.16, its integer and fractional parts, clamped to the plane
          f"fx{s}:cx{s}+((x-bx{s})*dx{s})+(shr[dx{s};1])-32768",
          f"fy{s}:cy{s}+((y-by{s})*dy{s})+(shr[dy{s};1])-32768",
          f"ax{s}:clamp[shr[fx{s};16];0;sw{s}-2]", f"ay{s}:clamp[shr[fy{s};16];0;sh{s}-2]",
          f"wx{s}:clamp[and[shr[fx{s};8];255]*(fx{s}>0-1);0;255]", f"wy{s}:clamp[and[shr[fy{s};8];255]*(fy{s}>0-1);0;255]",
          f"y00{s}:byte[{P};(ay{s}*sw{s})+ax{s}]", f"y10{s}:byte[{P};(ay{s}*sw{s})+ax{s}+1]",
          f"y01{s}:byte[{P};((ay{s}+1)*sw{s})+ax{s}]", f"y11{s}:byte[{P};((ay{s}+1)*sw{s})+ax{s}+1]",
          f"ly{s}:shr[(y00{s}*(256-wx{s})*(256-wy{s}))+(y10{s}*wx{s}*(256-wy{s}))+(y01{s}*(256-wx{s})*wy{s})+(y11{s}*wx{s}*wy{s})+32768;16]",
          # chroma, bilinear at half resolution: the sample centre in chroma space is half the luma centre
          f"gx{s}:(shr[fx{s}+32768;1])-32768", f"gy{s}:(shr[fy{s}+32768;1])-32768",
          f"qx{s}:clamp[shr[gx{s};16];0;(sw{s}%2)-2]", f"qy{s}:clamp[shr[gy{s};16];0;(sh{s}%2)-2]",
          f"vx{s}:clamp[and[shr[gx{s};8];255]*(gx{s}>0-1);0;255]", f"vy{s}:clamp[and[shr[gy{s};8];255]*(gy{s}>0-1);0;255]",
          f"ci{s}:(sw{s}*sh{s})+(qy{s}*sw{s})+2*qx{s}",
          f"u00{s}:byte[{P};ci{s}]", f"u10{s}:byte[{P};ci{s}+2]", f"u01{s}:byte[{P};ci{s}+sw{s}]", f"u11{s}:byte[{P};ci{s}+sw{s}+2]",
          f"v00{s}:byte[{P};ci{s}+1]", f"v10{s}:byte[{P};ci{s}+3]", f"v01{s}:byte[{P};ci{s}+sw{s}+1]", f"v11{s}:byte[{P};ci{s}+sw{s}+3]",
          f"cu{s}:(shr[(u00{s}*(256-vx{s})*(256-vy{s}))+(u10{s}*vx{s}*(256-vy{s}))+(u01{s}*(256-vx{s})*vy{s})+(u11{s}*vx{s}*vy{s})+32768;16])-128",
          f"cv{s}:(shr[(v00{s}*(256-vx{s})*(256-vy{s}))+(v10{s}*vx{s}*(256-vy{s}))+(v01{s}*(256-vx{s})*vy{s})+(v11{s}*vx{s}*vy{s})+32768;16])-128",
          f"k{s}:298*ly{s}-16",
          f"rr{s}:clamp[shr[k{s}+(459*cv{s})+128;8];0;255]",
          f"gg{s}:clamp[shr[k{s}-(55*cu{s})+(136*cv{s})-128;8];0;255]",
          f"bb{s}:clamp[shr[k{s}+(541*cu{s})+128;8];0;255]",
          f"r:shr[(am{s}*rr{s})+((256-am{s})*r);8]",
          f"g:shr[(am{s}*gg{s})+((256-am{s})*g);8]",
          f"b:shr[(am{s}*bb{s})+((256-am{s})*b);8]",
          "end"]
L += ["/ the overlay, premultiplied: o + below * (255 - a) / 255",
      "o:word[O;i]", "oa:and[shr[o;24];255]",
      "r:and[o;255]+((r*(255-oa))+127)%255",
      "g:and[shr[o;8];255]+((g*(255-oa))+127)%255",
      "b:and[shr[o;16];255]+((b*(255-oa))+127)%255",
      "px:pack[r;g;b;255]"]
open("kernels/composite.k", "w").write("\n".join(L) + "\n")

# composite_tex.k: the same, with each slot's planes as textures, as VideoToolbox's IOSurface-backed buffers wrap
# without a copy: Ys an R8 luma plane (texel = y), Us an RG8 chroma plane (texel = u | v << 8), both read as packed words
import re
X = []
for line in L:
    for s_ in range(SLOTS):
        P = f"P{s_}"
        # luma: byte[P;(ay*sw)+ax(+1)] and byte[P;((ay+1)*sw)+ax(+1)]
        line = re.sub(rf"byte\[{P};\(ay{s_}\*sw{s_}\)\+ax{s_}(\+1)?\]",
                      lambda m: f"and[tex[Y{s_};ax{s_}{m.group(1) or ''};ay{s_}];255]", line)
        line = re.sub(rf"byte\[{P};\(\(ay{s_}\+1\)\*sw{s_}\)\+ax{s_}(\+1)?\]",
                      lambda m: f"and[tex[Y{s_};ax{s_}{m.group(1) or ''};ay{s_}+1];255]", line)
        # chroma: byte[P;ci+k] for k in 0 1 2 3 sw sw+1 sw+2 sw+3 -> (qx + k%2... ) u/v of texel (qx (+1), qy (+1))
        def chroma(m, s=s_):
            k = m.group(1) or ""
            dx = "+1" if k in ("+2", "+3", "+sw%d+2" % s, "+sw%d+3" % s) else ""
            dy = "+1" if k.startswith(f"+sw{s}") else ""
            v = k in ("+1", "+3", f"+sw{s}+1", f"+sw{s}+3")
            t = f"tex[U{s};qx{s}{dx};qy{s}{dy}]"
            return f"and[shr[{t};8];255]" if v else f"and[{t};255]"
        line = re.sub(rf"byte\[{P};ci{s_}((?:\+sw{s_})?(?:\+\d)?)\]", chroma, line)
    if line.startswith("ci"):
        continue
    X.append(line)
X[0:4] = ["/ composite.k with each clip's NV12 planes as textures (Y0..Y3 R8 luma, U0..U3 RG8 chroma), as VideoToolbox's",
          "/ buffers wrap without a copy; the camera's target T, the clip parameters C and the overlay O as in composite.k.",
          "/ The same arithmetic, so the same pixels as composite.k on the same planes."]
assert not any("byte[P" in l for l in X), [l for l in X if "byte[P" in l]
open("kernels/composite_tex.k", "w").write("\n".join(X) + "\n")


src = open("kernels/nv12.k").read()
import re
# pixel reads come from the composite buffer F (w x h words); the frame tag still comes from the target texture
src = re.sub(r"tex\[T;([^;]+);([^\]]+)\]", lambda m: m.group(0) if m.group(1) == "8" and m.group(2) == "h" else f"word[F;(({m.group(2)})*w)+{m.group(1)}]", src)
src = src.replace("/ RGBA (texture T,", "/ RGBA (buffer F, w x h words; the frame tag is texel (8, h) of texture T;", 1)
open("kernels/nv12buf.k", "w").write(src)

# sketch.k: the whole edit at proxy size in one dispatch. Thread i is pixel p of frame fi; frame fi's 53 parameter
# words in Q are four slots of (the composite's 12 + the byte offset of its plane in PP), then the word offset of its
# overlay in OO (-1 for none). The background is the scalar colour bgc.
S = ["/ The whole edit at proxy size (pw x ph) in one dispatch: thread i is pixel p of frame fi. Planes are packed in",
     "/ PP (NV12 bytes), overlays in OO (premultiplied RGBA words); frame fi's parameters are words fi*53.. of Q.",
     "np:pw*ph", "fi:i%np", "p:i!np", "x:p!pw", "y:p%pw", "q:fi*53",
     "r:and[bgc;255]", "g:and[shr[bgc;8];255]", "b:and[shr[bgc;16];255]"]
for s in range(SLOTS):
    c = lambda k: f"word[Q;q+{s * 13 + k}]"
    S += [f"on{s}:{c(0)}", f"bx{s}:{c(1)}", f"by{s}:{c(2)}", f"ex{s}:{c(3)}", f"ey{s}:{c(4)}",
          f"in{s}:on{s}*(x>bx{s}-1)*(x<ex{s})*(y>by{s}-1)*(y<ey{s})",
          f"when in{s}",
          f"sw{s}:{c(5)}", f"sh{s}:{c(6)}", f"cx{s}:{c(7)}", f"cy{s}:{c(8)}", f"dx{s}:{c(9)}", f"dy{s}:{c(10)}", f"al{s}:{c(11)}", f"po{s}:{c(12)}",
          f"fx{s}:cx{s}+((x-bx{s})*dx{s})+(shr[dx{s};1])-32768",
          f"fy{s}:cy{s}+((y-by{s})*dy{s})+(shr[dy{s};1])-32768",
          f"ax{s}:clamp[shr[fx{s};16];0;sw{s}-2]", f"ay{s}:clamp[shr[fy{s};16];0;sh{s}-2]",
          f"wx{s}:clamp[and[shr[fx{s};8];255]*(fx{s}>0-1);0;255]", f"wy{s}:clamp[and[shr[fy{s};8];255]*(fy{s}>0-1);0;255]",
          f"y00{s}:byte[PP;po{s}+(ay{s}*sw{s})+ax{s}]", f"y10{s}:byte[PP;po{s}+(ay{s}*sw{s})+ax{s}+1]",
          f"y01{s}:byte[PP;po{s}+((ay{s}+1)*sw{s})+ax{s}]", f"y11{s}:byte[PP;po{s}+((ay{s}+1)*sw{s})+ax{s}+1]",
          f"ly{s}:shr[(y00{s}*(256-wx{s})*(256-wy{s}))+(y10{s}*wx{s}*(256-wy{s}))+(y01{s}*(256-wx{s})*wy{s})+(y11{s}*wx{s}*wy{s})+32768;16]",
          f"gx{s}:(shr[fx{s}+32768;1])-32768", f"gy{s}:(shr[fy{s}+32768;1])-32768",
          f"qx{s}:clamp[shr[gx{s};16];0;(sw{s}%2)-2]", f"qy{s}:clamp[shr[gy{s};16];0;(sh{s}%2)-2]",
          f"vx{s}:clamp[and[shr[gx{s};8];255]*(gx{s}>0-1);0;255]", f"vy{s}:clamp[and[shr[gy{s};8];255]*(gy{s}>0-1);0;255]",
          f"ci{s}:po{s}+(sw{s}*sh{s})+(qy{s}*sw{s})+2*qx{s}",
          f"u00{s}:byte[PP;ci{s}]", f"u10{s}:byte[PP;ci{s}+2]", f"u01{s}:byte[PP;ci{s}+sw{s}]", f"u11{s}:byte[PP;ci{s}+sw{s}+2]",
          f"v00{s}:byte[PP;ci{s}+1]", f"v10{s}:byte[PP;ci{s}+3]", f"v01{s}:byte[PP;ci{s}+sw{s}+1]", f"v11{s}:byte[PP;ci{s}+sw{s}+3]",
          f"cu{s}:(shr[(u00{s}*(256-vx{s})*(256-vy{s}))+(u10{s}*vx{s}*(256-vy{s}))+(u01{s}*(256-vx{s})*vy{s})+(u11{s}*vx{s}*vy{s})+32768;16])-128",
          f"cv{s}:(shr[(v00{s}*(256-vx{s})*(256-vy{s}))+(v10{s}*vx{s}*(256-vy{s}))+(v01{s}*(256-vx{s})*vy{s})+(v11{s}*vx{s}*vy{s})+32768;16])-128",
          f"k{s}:298*ly{s}-16",
          f"r:shr[(al{s}*clamp[shr[k{s}+(459*cv{s})+128;8];0;255])+((256-al{s})*r);8]",
          f"g:shr[(al{s}*clamp[shr[k{s}-(55*cu{s})+(136*cv{s})-128;8];0;255])+((256-al{s})*g);8]",
          f"b:shr[(al{s}*clamp[shr[k{s}+(541*cu{s})+128;8];0;255])+((256-al{s})*b);8]",
          "end"]
S += ["oo:word[Q;q+52]",
      "when oo>0-1",
      "o:word[OO;oo+p]", "oa:and[shr[o;24];255]",
      "r:and[o;255]+((r*(255-oa))+127)%255",
      "g:and[shr[o;8];255]+((g*(255-oa))+127)%255",
      "b:and[shr[o;16];255]+((b*(255-oa))+127)%255",
      "end",
      "px:pack[r;g;b;255]"]
open("kernels/sketch.k", "w").write("\n".join(S) + "\n")
