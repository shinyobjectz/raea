/ RGBA (buffer F, w x h words; the frame tag is texel (8, h) of texture T) to P010, BT.709 limited range at 10 bits:
/ each sample a 16-bit word with the value in its top 10 bits, as CoreVideo's 420YpCbCr10BiPlanarVideoRange holds it.
/ Luma in 16.16 fixed point, chroma from the 2x2 block's sums (>> 18); both round half up. Every one of the 16.7M
/ 8-bit colours comes back exact through a float BT.709 decode (kgpu's p010 tests), which 8 bits can't do.
/ One output word per thread: two luma samples, or one Cb Cr pair, or, last, the tag texel.
ny:(w*h)%2
nuv:(w*h)%4
/ luma: pixels 2i and 2i+1
p:(2*i)&(w*h)-2
ly:p%w
lx:p!w
c0:word[F;((ly)*w)+lx]
y0:64+shr[(47864*and[c0;255])+(161017*and[shr[c0;8];255])+(16255*and[shr[c0;16];255])+32768;16]
c1:word[F;((ly)*w)+lx+1]
y1:64+shr[(47864*and[c1;255])+(161017*and[shr[c1;8];255])+(16255*and[shr[c1;16];255])+32768;16]
yw:or[shl[y0;6];shl[y1;22]]
/ chroma: sample j of the (w/2) x (h/2) grid, j counting from the first chroma word
j:0|(i-ny)&nuv-1
cr:j%(w%2)
cc:j!(w%2)
x0:2*cc
t0:word[F;((2*cr)*w)+x0]
t1:word[F;((2*cr)*w)+x0+1]
t2:word[F;(((2*cr)+1)*w)+x0]
t3:word[F;(((2*cr)+1)*w)+x0+1]
r:and[t0;255]+and[t1;255]+and[t2;255]+and[t3;255]
g:and[shr[t0;8];255]+and[shr[t1;8];255]+and[shr[t2;8];255]+and[shr[t3;8];255]
b:and[shr[t0;16];255]+and[shr[t1;16];255]+and[shr[t2;16];255]+and[shr[t3;16];255]
u:512+shr[(-26383*r)+(-88755*g)+(115138*b)+131072;18]
v:512+shr[(115138*r)+(-104581*g)+(-10557*b)+131072;18]
uvw:or[shl[u;6];shl[v;22]]
px:sel[i<ny;yw;sel[i<ny+nuv;uvw;tex[T;8;h]]]
