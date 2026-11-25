`timescale 1 ns / 1 ps
// Plays into an RF-DAC stream: either one I/Q value repeated (the converter's mixer then makes a tone at its NCO
// frequency), or a waveform from a dual-port block RAM, looped. The stream is continuous: tvalid stays high while the
// module is out of reset, and the data is zero while disabled.
//
// `ctrl` and `iq` come from another clock domain and are meant to change while the player is disabled:
//   ctrl[0]      enable
//   ctrl[1]      1: play the memory, 0: repeat `iq`
//   ctrl[31:16]  last word of the loop (memory mode): 0 plays one word, 4095 all of a 4096-word memory
//   iq           {Q[15:0], I[15:0]}, repeated through the stream word
// A stream word holds DATA_BITS/32 I/Q pairs, the first in the low bits.
module hrx_player #(
  parameter integer DATA_BITS = 256,
  parameter integer BYTE_SHIFT = 5,  // log2(DATA_BITS / 8): the memory is byte addressed
  parameter integer READ_LATENCY = 1 // clocks from an address to its data, 1 or 2
) (
  (* X_INTERFACE_INFO = "xilinx.com:signal:clock:1.0 clk CLK" *)
  (* X_INTERFACE_PARAMETER = "ASSOCIATED_BUSIF m_axis, ASSOCIATED_RESET aresetn" *)
  input  wire                    clk,
  (* X_INTERFACE_INFO = "xilinx.com:signal:reset:1.0 aresetn RST" *)
  (* X_INTERFACE_PARAMETER = "POLARITY ACTIVE_LOW" *)
  input  wire                    aresetn,
  input  wire [31:0]             ctrl,
  input  wire [31:0]             iq,

  (* X_INTERFACE_INFO = "xilinx.com:interface:bram:1.0 bram CLK" *)
  (* X_INTERFACE_PARAMETER = "XIL_INTERFACENAME bram, MODE Master, MASTER_TYPE BRAM_CTRL, MEM_WIDTH 256, MEM_SIZE 131072, READ_WRITE_MODE READ_WRITE, READ_LATENCY 1" *)
  output wire                    bram_clk,
  (* X_INTERFACE_INFO = "xilinx.com:interface:bram:1.0 bram RST" *)
  output wire                    bram_rst,
  (* X_INTERFACE_INFO = "xilinx.com:interface:bram:1.0 bram EN" *)
  output wire                    bram_en,
  (* X_INTERFACE_INFO = "xilinx.com:interface:bram:1.0 bram WE" *)
  output wire [DATA_BITS/8-1:0]  bram_we,
  (* X_INTERFACE_INFO = "xilinx.com:interface:bram:1.0 bram ADDR" *)
  output wire [31:0]             bram_addr,
  (* X_INTERFACE_INFO = "xilinx.com:interface:bram:1.0 bram DIN" *)
  output wire [DATA_BITS-1:0]    bram_din,
  (* X_INTERFACE_INFO = "xilinx.com:interface:bram:1.0 bram DOUT" *)
  input  wire [DATA_BITS-1:0]    bram_dout,

  output reg  [DATA_BITS-1:0]    m_axis_tdata,
  output reg                     m_axis_tvalid,
  input  wire                    m_axis_tready
);
  wire [31:0] ctrl_s, iq_s;
  xpm_cdc_array_single #(.DEST_SYNC_FF(3), .SRC_INPUT_REG(0), .WIDTH(32)) sync_ctrl
    (.src_clk(1'b0), .src_in(ctrl), .dest_clk(clk), .dest_out(ctrl_s));
  xpm_cdc_array_single #(.DEST_SYNC_FF(3), .SRC_INPUT_REG(0), .WIDTH(32)) sync_iq
    (.src_clk(1'b0), .src_in(iq), .dest_clk(clk), .dest_out(iq_s));

  wire        enable = ctrl_s[0];
  wire        memory = ctrl_s[1];
  wire [15:0] last   = ctrl_s[31:16];

  reg [15:0] index;
  always @(posedge clk) begin
    if (!aresetn || !enable || !memory) index <= 16'd0;
    else if (index >= last)             index <= 16'd0;
    else                                index <= index + 16'd1;
  end

  assign bram_clk  = clk;
  assign bram_rst  = 1'b0;
  assign bram_en   = 1'b1;
  assign bram_we   = {(DATA_BITS/8){1'b0}};
  assign bram_din  = {DATA_BITS{1'b0}};
  assign bram_addr = {16'd0, index} << BYTE_SHIFT;

  // Which clock the memory's answer to the first address arrives on.
  reg [1:0] reading;
  always @(posedge clk) reading <= {reading[0], aresetn && enable && memory};

  always @(posedge clk) begin
    m_axis_tvalid <= aresetn;
    if (!aresetn || !enable) m_axis_tdata <= {DATA_BITS{1'b0}};
    else if (!memory)        m_axis_tdata <= {(DATA_BITS/32){iq_s}};
    else if (reading[READ_LATENCY-1]) m_axis_tdata <= bram_dout;
    else                     m_axis_tdata <= {DATA_BITS{1'b0}};
  end
endmodule
