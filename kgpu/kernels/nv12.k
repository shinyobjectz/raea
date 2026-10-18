/ RGBA (texture T, w x h, plus a frame-tag texel at (8, h)) to NV12, BT.709 limited range in 8.8 fixed point.
/ One output word per thread: four luma bytes, or two interleaved chroma pairs (each the mean of a 2x2 block),
/ or, last, the tag texel. Coordinates are clamped so the branch not taken stays in bounds.
ny:(w*h)%4
nuv:(w*h)%8
/ luma: pixels 4i..4i+3
p:(4*i)&(w*h)-4
ly:p%w
lx:p!w
c0:tex[T;lx+0;ly]
y0:clamp[16+shr[(47*and[c0;255])+(157*and[shr[c0;8];255])+(16*and[shr[c0;16];255])+128;8];0;255]
c1:tex[T;lx+1;ly]
y1:clamp[16+shr[(47*and[c1;255])+(157*and[shr[c1;8];255])+(16*and[shr[c1;16];255])+128;8];0;255]
c2:tex[T;lx+2;ly]
y2:clamp[16+shr[(47*and[c2;255])+(157*and[shr[c2;8];255])+(16*and[shr[c2;16];255])+128;8];0;255]
c3:tex[T;lx+3;ly]
y3:clamp[16+shr[(47*and[c3;255])+(157*and[shr[c3;8];255])+(16*and[shr[c3;16];255])+128;8];0;255]
yw:or[or[y0;shl[y1;8]];or[shl[y2;16];shl[y3;24]]]
/ chroma: samples 2j and 2j+1 of the (w/2) x (h/2) grid, j counting from the first chroma word
j:0|(i-ny)&nuv-1
s:2*j
cr:s%(w%2)
cc:s!(w%2)
x0:2*cc+0
t00:tex[T;x0;2*cr]
t01:tex[T;x0+1;2*cr]
t02:tex[T;x0;(2*cr)+1]
t03:tex[T;x0+1;(2*cr)+1]
r0:and[t00;255]+and[t01;255]+and[t02;255]+and[t03;255]
g0:and[shr[t00;8];255]+and[shr[t01;8];255]+and[shr[t02;8];255]+and[shr[t03;8];255]
b0:and[shr[t00;16];255]+and[shr[t01;16];255]+and[shr[t02;16];255]+and[shr[t03;16];255]
u0:clamp[128+shr[(-26*r0)+(-87*g0)+(112*b0)+512;10];0;255]
v0:clamp[128+shr[(112*r0)+(-102*g0)+(-10*b0)+512;10];0;255]
x1:2*cc+1
t10:tex[T;x1;2*cr]
t11:tex[T;x1+1;2*cr]
t12:tex[T;x1;(2*cr)+1]
t13:tex[T;x1+1;(2*cr)+1]
r1:and[t10;255]+and[t11;255]+and[t12;255]+and[t13;255]
g1:and[shr[t10;8];255]+and[shr[t11;8];255]+and[shr[t12;8];255]+and[shr[t13;8];255]
b1:and[shr[t10;16];255]+and[shr[t11;16];255]+and[shr[t12;16];255]+and[shr[t13;16];255]
u1:clamp[128+shr[(-26*r1)+(-87*g1)+(112*b1)+512;10];0;255]
v1:clamp[128+shr[(112*r1)+(-102*g1)+(-10*b1)+512;10];0;255]
uvw:or[or[u0;shl[v0;8]];or[shl[u1;16];shl[v1;24]]]
px:sel[i<ny;yw;sel[i<ny+nuv;uvw;tex[T;8;h]]]
