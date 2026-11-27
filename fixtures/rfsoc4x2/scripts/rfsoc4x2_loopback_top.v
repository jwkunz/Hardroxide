`timescale 1 ps / 1 ps
// Top level of rfsoc4x2_loopback: no pins, the interfaces are wired to each other here. An I2C line is open-drain: it
// is low when any master drives it low (t=0 and o=0). The SPI master reads what it writes; its slave-select input stays
// high (a low there is a mode fault).
module design_1_wrapper ();
  wire UART_0_rxd, UART_0_txd, UART_1_rxd, UART_1_txd;
  wire IIC_0_scl_i, IIC_0_scl_o, IIC_0_scl_t, IIC_0_sda_i, IIC_0_sda_o, IIC_0_sda_t;
  wire IIC_1_scl_i, IIC_1_scl_o, IIC_1_scl_t, IIC_1_sda_i, IIC_1_sda_o, IIC_1_sda_t;
  wire SPI_0_0_io0_i, SPI_0_0_io0_o, SPI_0_0_io0_t, SPI_0_0_io1_i, SPI_0_0_io1_o, SPI_0_0_io1_t;
  wire SPI_0_0_sck_i, SPI_0_0_sck_o, SPI_0_0_sck_t, SPI_0_0_ss_t;
  wire [0:0] SPI_0_0_ss_i, SPI_0_0_ss_o;
  assign UART_1_rxd = UART_0_txd;
  assign UART_0_rxd = UART_1_txd;
  wire scl_line = (IIC_0_scl_t | IIC_0_scl_o) & (IIC_1_scl_t | IIC_1_scl_o);
  wire sda_line = (IIC_0_sda_t | IIC_0_sda_o) & (IIC_1_sda_t | IIC_1_sda_o);
  assign IIC_0_scl_i = scl_line;
  assign IIC_1_scl_i = scl_line;
  assign IIC_0_sda_i = sda_line;
  assign IIC_1_sda_i = sda_line;
  assign SPI_0_0_io1_i = SPI_0_0_io0_o;
  assign SPI_0_0_io0_i = SPI_0_0_io0_o;
  assign SPI_0_0_sck_i = SPI_0_0_sck_o;
  assign SPI_0_0_ss_i = 1'b1;
  design_1 design_1_i
       (.IIC_0_scl_i(IIC_0_scl_i), .IIC_0_scl_o(IIC_0_scl_o), .IIC_0_scl_t(IIC_0_scl_t),
        .IIC_0_sda_i(IIC_0_sda_i), .IIC_0_sda_o(IIC_0_sda_o), .IIC_0_sda_t(IIC_0_sda_t),
        .IIC_1_scl_i(IIC_1_scl_i), .IIC_1_scl_o(IIC_1_scl_o), .IIC_1_scl_t(IIC_1_scl_t),
        .IIC_1_sda_i(IIC_1_sda_i), .IIC_1_sda_o(IIC_1_sda_o), .IIC_1_sda_t(IIC_1_sda_t),
        .SPI_0_0_io0_i(SPI_0_0_io0_i), .SPI_0_0_io0_o(SPI_0_0_io0_o), .SPI_0_0_io0_t(SPI_0_0_io0_t),
        .SPI_0_0_io1_i(SPI_0_0_io1_i), .SPI_0_0_io1_o(SPI_0_0_io1_o), .SPI_0_0_io1_t(SPI_0_0_io1_t),
        .SPI_0_0_sck_i(SPI_0_0_sck_i), .SPI_0_0_sck_o(SPI_0_0_sck_o), .SPI_0_0_sck_t(SPI_0_0_sck_t),
        .SPI_0_0_ss_i(SPI_0_0_ss_i), .SPI_0_0_ss_o(SPI_0_0_ss_o), .SPI_0_0_ss_t(SPI_0_0_ss_t),
        .UART_0_rxd(UART_0_rxd), .UART_0_txd(UART_0_txd),
        .UART_1_rxd(UART_1_rxd), .UART_1_txd(UART_1_txd));
endmodule
