/ Energy scans (rdb/docs/energy.feature): a column's windowed mean over 2r+1 rows, edges clamped. C is Q16; thread i
/ is a row.
v:sum[j;(2*r)+1;word[C;clamp[(i-r)+j;0;n-1]]]%((2*r)+1)
