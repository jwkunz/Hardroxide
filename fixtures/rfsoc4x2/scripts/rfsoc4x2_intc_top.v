`timescale 1 ps / 1 ps
// Top level of rfsoc4x2_intc: no pins. An I2C line is open-drain: low when the master drives it low (t=0 and o=0),
// otherwise pulled high.
module design_1_wrapper ();
  wire IIC_0_scl_i, IIC_0_scl_o, IIC_0_scl_t, IIC_0_sda_i, IIC_0_sda_o, IIC_0_sda_t;
  assign IIC_0_scl_i = IIC_0_scl_t | IIC_0_scl_o;
  assign IIC_0_sda_i = IIC_0_sda_t | IIC_0_sda_o;
  design_1 design_1_i
       (.IIC_0_scl_i(IIC_0_scl_i), .IIC_0_scl_o(IIC_0_scl_o), .IIC_0_scl_t(IIC_0_scl_t),
        .IIC_0_sda_i(IIC_0_sda_i), .IIC_0_sda_o(IIC_0_sda_o), .IIC_0_sda_t(IIC_0_sda_t));
endmodule
