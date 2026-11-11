// HLS fixture with memory-mapped masters: out[i] = in[i] * k for i < n.
// Pointer arguments become 64-bit address registers on the AXI-Lite port.
extern "C" void scale(const int *in, int *out, int n, int k) {
#pragma HLS INTERFACE m_axi port=in offset=slave bundle=gmem0
#pragma HLS INTERFACE m_axi port=out offset=slave bundle=gmem1
#pragma HLS INTERFACE s_axilite port=in
#pragma HLS INTERFACE s_axilite port=out
#pragma HLS INTERFACE s_axilite port=n
#pragma HLS INTERFACE s_axilite port=k
#pragma HLS INTERFACE s_axilite port=return
    for (int i = 0; i < n; i++) {
#pragma HLS PIPELINE II=1
        out[i] = in[i] * k;
    }
}
