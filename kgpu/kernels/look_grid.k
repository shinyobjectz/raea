/ Look features from the sketch (proxy frames pw x ph, RGBA words in S): thread i is (frame, cell, channel) with
/ 9 x 16 cells and 3 channels (luma r+g+b, red-green, yellow-blue); each sums its cell's pixels.
cw:pw%9
ch:ph%16
f:i%432
c:(i!432)%3
k:i!3
cx:c!9
cy:c%9
base:(f*pw*ph)+(cy*ch*pw)+cx*cw
v:sum[j;cw*ch;sel[k=0;and[word[S;base+((j%cw)*pw)+j!cw];255]+and[shr[word[S;base+((j%cw)*pw)+j!cw];8];255]+and[shr[word[S;base+((j%cw)*pw)+j!cw];16];255];sel[k=1;and[word[S;base+((j%cw)*pw)+j!cw];255]-and[shr[word[S;base+((j%cw)*pw)+j!cw];8];255];(shr[and[word[S;base+((j%cw)*pw)+j!cw];255]+and[shr[word[S;base+((j%cw)*pw)+j!cw];8];255];1])-and[shr[word[S;base+((j%cw)*pw)+j!cw];16];255]]]]
