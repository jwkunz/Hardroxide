// Minimal HLS fixture: out = a + b, plus a scalar accumulator, all via AXI-Lite.
extern "C" void vadd(int a, int b, int *out) {
#pragma HLS INTERFACE s_axilite port=a
#pragma HLS INTERFACE s_axilite port=b
#pragma HLS INTERFACE s_axilite port=out
#pragma HLS INTERFACE s_axilite port=return
    *out = a + b;
}
