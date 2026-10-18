/ Hamming distance between the look hypervectors (64 words each, in H) of every pair of n frames.
a:i%n
b:i!n
d:sum[k;64;pop[xor[word[H;(a*64)+k];word[H;(b*64)+k]]]]
