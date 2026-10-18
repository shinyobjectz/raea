/ CV columns: each row's change from the row before (row 0 is zeros), per dimension. Thread i is (row, dimension).
r:i%d
j:i!d
v:sel[r>0;word[X;(r*d)+j]-word[X;((r-1)*d)+j];0]
