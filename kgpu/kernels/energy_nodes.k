/ Energy scans: the smoothed curve's salient nodes per row: 1 a peak (payoff), 2 a trough (breath), 3 an inflection
/ (the second difference changes sign), 0 a slope; the first two rows and the last are 0. S is the smoothed column.
p:word[S;clamp[i-2;0;n-1]]
a:word[S;clamp[i-1;0;n-1]]
b:word[S;i]
c:word[S;clamp[i+1;0;n-1]]
pk:(b>a)*((c>b)=0)
th:(a>b)*((b>c)=0)
s1:(b+p)-(2*a)
s2:(c+a)-(2*b)
inf:((s1>0)*(0>s2))+((0>s1)*(s2>0))
edge:(2>i)+(i>(n-2))
v:sel[edge;0;sel[pk;1;sel[th;2;sel[inf;3;0]]]]
