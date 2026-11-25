`timescale 1 ns / 1 ps
// Test bench for hrx_player and hrx_capture. Run with scripts/sim_rf.sh; prints "TB PASS" or the first failure.
module tb_rf;
  reg clk = 0, bus = 0, aresetn = 0;
  always #1.6276 clk = ~clk;   // the converters' 307.2 MHz stream clock
  always #5 bus = ~bus;        // the processor's 100 MHz

  integer errors = 0;
  task check(input ok, input [511:0] what);
    if (!ok) begin errors = errors + 1; $display("TB FAIL at %0t: %0s", $time, what); end
  endtask

  // ---- player, with a memory that answers MEM_LATENCY clocks after the address ----
  parameter integer MEM_LATENCY = 1;
  reg  [31:0]  pctrl = 0, pconst = 0;
  wire [31:0]  paddr;
  wire [255:0] pdata;
  wire         pvalid;
  reg  [255:0] mem [0:15];
  reg  [255:0] q1, q2;
  integer i;
  initial for (i = 0; i < 16; i = i + 1) mem[i] = {8{32'h1000_0000 + i}};
  always @(posedge clk) begin q1 <= mem[paddr[8:5]]; q2 <= q1; end

  hrx_player #(.READ_LATENCY(MEM_LATENCY)) player (.clk(clk), .aresetn(aresetn), .ctrl(pctrl), .iq(pconst), .bram_clk(), .bram_rst(),
    .bram_en(), .bram_we(), .bram_addr(paddr), .bram_din(), .bram_dout(MEM_LATENCY == 1 ? q1 : q2), .m_axis_tdata(pdata),
    .m_axis_tvalid(pvalid), .m_axis_tready(1'b1));

  // ---- capture ----
  reg  [31:0]  cctrl = 0;
  wire [31:0]  status;
  reg  [127:0] n0 = 0, n1 = 0;
  reg          v0 = 0, v1 = 0, ready = 1;
  wire [127:0] cdata;
  wire         cvalid, clast;
  integer      gaps = 0, stall = 0;
  // Two streams of counting words, told apart by bit 127; with gaps when asked to.
  always @(posedge clk) begin
    v0 <= gaps ? ($random & 1) : 1'b1;
    v1 <= gaps ? ($random & 1) : 1'b1;
    if (v0) n0 <= n0 + 1;
    if (v1) n1 <= n1 + 1;
    ready <= stall ? (($random & 7) == 0) : 1'b1;
  end

  hrx_capture capture (.clk(clk), .aresetn(aresetn), .ctrl(cctrl), .status_clk(bus), .status(status),
    .s0_axis_tdata(n0), .s0_axis_tvalid(v0), .s0_axis_tready(),
    .s1_axis_tdata({1'b1, n1[126:0]}), .s1_axis_tvalid(v1), .s1_axis_tready(),
    .m_axis_tdata(cdata), .m_axis_tvalid(cvalid), .m_axis_tlast(clast), .m_axis_tready(ready));

  integer      count = 0, lasts = 0, skips = 0;
  reg          src, first = 1;
  reg  [127:0] prev;
  always @(posedge clk) if (aresetn && cvalid && ready) begin
    if (!first) begin
      check(cdata[127] == src, "a word from the other input");
      check(cdata[126:0] > prev[126:0], "words out of order or repeated");
      if (cdata[126:0] != prev[126:0] + 1) skips = skips + 1;
    end
    src <= cdata[127]; prev <= cdata; first <= 0;
    count = count + 1;
    if (clast) lasts = lasts + 1;
  end

  task run_capture(input sel, input integer words);
    integer t;
    begin
      count = 0; lasts = 0; skips = 0; first = 1;
      @(posedge bus) cctrl <= {words[23:0], 6'd0, sel, 1'b0};
      repeat (5) @(posedge bus);
      cctrl <= {words[23:0], 6'd0, sel, 1'b1};
      repeat (10) @(posedge bus);
      t = 0;
      while (!status[1] && t < 20000) begin @(posedge bus); t = t + 1; end
      repeat (50) @(posedge bus);
      check(status[1] && !status[0], "capture did not finish");
      check(count == words, "wrong number of words");
      check(lasts == 1, "not exactly one tlast");
      check(first == 0 && src == sel, "wrong input");
    end
  endtask

  integer k, seen;
  reg [31:0] word, expect_word;
  initial begin
    repeat (20) @(posedge clk); aresetn <= 1; repeat (20) @(posedge clk);
    // Disabled: valid, zero.
    check(pvalid && pdata == 0, "disabled player is not sending zeros");
    // Constant.
    pconst <= 32'h1234_5678; pctrl <= 32'h1; repeat (20) @(posedge clk);
    check(pdata == {8{32'h1234_5678}}, "constant not repeated");
    // Memory, looping over words 0..5: consecutive outputs follow the memory and wrap.
    pctrl <= 0; repeat (20) @(posedge clk);
    pctrl <= {16'd5, 16'h3}; repeat (40) @(posedge clk);
    @(negedge clk); word = pdata[31:0];
    check(word >= 32'h1000_0000 && word <= 32'h1000_0005, "memory data not seen");
    for (k = 0; k < 40; k = k + 1) begin
      expect_word = (word == 32'h1000_0005) ? 32'h1000_0000 : word + 1;
      @(negedge clk); word = pdata[31:0];
      check(word == expect_word, "memory loop out of order");
      check(pdata == {8{word}}, "word not whole");
    end
    // The first words after enabling are zeros, then word 0 (nothing stale).
    pctrl <= 0; repeat (20) @(posedge clk);
    pctrl <= {16'd15, 16'h3}; seen = 0;
    for (k = 0; k < 40 && !seen; k = k + 1) begin
      @(negedge clk);
      if (pdata != 0) begin seen = 1; check(pdata[31:0] == 32'h1000_0000, "playback does not start at word 0"); end
    end
    check(seen, "nothing played");
    pctrl <= 0; repeat (20) @(posedge clk);
    check(pdata == 0, "disable does not silence");

    // Captures: both inputs, one word, many words, gaps in the input.
    run_capture(0, 1);
    check(!status[2], "dropped flag without back-pressure");
    run_capture(0, 100);  check(skips == 0 && !status[2], "lost words without back-pressure");
    run_capture(1, 257);  check(skips == 0 && !status[2], "lost words on input 1");
    gaps = 1;
    run_capture(0, 200);  check(skips == 0 && !status[2], "lost words with gaps");
    // Back-pressure: the count and tlast still come out right, the loss is flagged.
    gaps = 0; stall = 1;
    run_capture(1, 64);   check(skips > 0 && status[2], "loss not flagged");
    stall = 0;
    run_capture(0, 32);   check(!status[2], "flag not cleared by the next capture");
    // A zero-word request does nothing.
    count = 0;
    @(posedge bus) cctrl <= 0; repeat (5) @(posedge bus); cctrl <= 32'h1; repeat (50) @(posedge bus);
    check(count == 0 && !status[0], "zero-word capture ran");

    if (errors == 0) $display("TB PASS"); else $display("TB FAIL: %0d error(s)", errors);
    $finish;
  end
  initial begin #2_000_000; $display("TB FAIL: timeout"); $finish; end
endmodule
