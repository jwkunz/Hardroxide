`timescale 1 ns / 1 ps
// Records two RF-ADC streams side by side: passes a requested number of words to the output, each holding one word of
// both inputs from the same clock (s0 in the low half, s1 in the high half), and marks the last with tlast (so a DMA
// finishes). The converters cannot be held up: their tready stays high. A word that finds the output still occupied is
// dropped and recorded, and so is a clock on which only one input has a word.
//
// `ctrl` comes from another clock domain:
//   ctrl[0]      a rising edge starts a capture (ignored while one is running)
//   ctrl[7:1]    unused
//   ctrl[31:8]   how many output words to capture (0 does nothing; set before the start edge)
// `status`, synchronised to `status_clk`:
//   bit 0 busy, bit 1 done (since the last start), bit 2 something was dropped (since the last start),
//   bit 3 follows ctrl[0] once the start edge has been seen
module hrx_capture #(
  parameter integer DATA_BITS = 128
) (
  (* X_INTERFACE_INFO = "xilinx.com:signal:clock:1.0 clk CLK" *)
  (* X_INTERFACE_PARAMETER = "ASSOCIATED_BUSIF s0_axis:s1_axis:m_axis, ASSOCIATED_RESET aresetn" *)
  input  wire                  clk,
  (* X_INTERFACE_INFO = "xilinx.com:signal:reset:1.0 aresetn RST" *)
  (* X_INTERFACE_PARAMETER = "POLARITY ACTIVE_LOW" *)
  input  wire                  aresetn,
  input  wire [31:0]           ctrl,
  (* X_INTERFACE_INFO = "xilinx.com:signal:clock:1.0 status_clk CLK" *)
  input  wire                  status_clk,
  output wire [31:0]           status,

  input  wire [DATA_BITS-1:0]  s0_axis_tdata,
  input  wire                  s0_axis_tvalid,
  output wire                  s0_axis_tready,
  input  wire [DATA_BITS-1:0]  s1_axis_tdata,
  input  wire                  s1_axis_tvalid,
  output wire                  s1_axis_tready,

  output reg  [2*DATA_BITS-1:0] m_axis_tdata,
  output reg                   m_axis_tvalid,
  output reg                   m_axis_tlast,
  input  wire                  m_axis_tready
);
  wire [31:0] ctrl_s;
  xpm_cdc_array_single #(.DEST_SYNC_FF(3), .SRC_INPUT_REG(0), .WIDTH(32)) sync_ctrl
    (.src_clk(1'b0), .src_in(ctrl), .dest_clk(clk), .dest_out(ctrl_s));

  assign s0_axis_tready = 1'b1;
  assign s1_axis_tready = 1'b1;

  reg        start_seen, busy, done, dropped;
  reg [23:0] left;

  // The start bit is delayed by one more clock than the other bits are needed, so a count written together with the
  // edge has settled.
  reg        start_d;
  wire       start = start_d && !start_seen;

  wire       both = s0_axis_tvalid && s1_axis_tvalid;
  wire       any  = s0_axis_tvalid || s1_axis_tvalid;
  wire       free = !m_axis_tvalid || m_axis_tready;

  always @(posedge clk) begin
    if (!aresetn) begin
      start_d <= 1'b0; start_seen <= 1'b0; busy <= 1'b0; done <= 1'b0; dropped <= 1'b0; left <= 24'd0;
      m_axis_tvalid <= 1'b0; m_axis_tlast <= 1'b0; m_axis_tdata <= {2*DATA_BITS{1'b0}};
    end else begin
      start_d    <= ctrl_s[0];
      start_seen <= start_d;
      if (m_axis_tvalid && m_axis_tready) m_axis_tvalid <= 1'b0;
      if (busy) begin
        if (any) begin
          if (both && free) begin
            m_axis_tdata  <= {s1_axis_tdata, s0_axis_tdata};
            m_axis_tvalid <= 1'b1;
            m_axis_tlast  <= (left == 24'd1);
            left          <= left - 24'd1;
            if (left == 24'd1) begin busy <= 1'b0; done <= 1'b1; end
          end else begin
            dropped <= 1'b1;
          end
        end
      end else if (start && ctrl_s[31:8] != 24'd0) begin
        busy <= 1'b1; done <= 1'b0; dropped <= 1'b0;
        left <= ctrl_s[31:8];
      end
    end
  end

  wire [3:0] status_s;
  xpm_cdc_array_single #(.DEST_SYNC_FF(3), .SRC_INPUT_REG(1), .WIDTH(4)) sync_status
    (.src_clk(clk), .src_in({start_seen, dropped, done, busy}), .dest_clk(status_clk), .dest_out(status_s));
  assign status = {28'd0, status_s};
endmodule
