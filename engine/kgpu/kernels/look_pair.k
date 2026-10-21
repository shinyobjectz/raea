/ How far each changed frame's look moved: thread i compares the new look of changed frame i (HN, 64 words each) with
/ the old look of the frame it is (IX[i], into HO).
d:sum[k;64;pop[xor[word[HO;(word[IX;i]*64)+k];word[HN;(i*64)+k]]]]
