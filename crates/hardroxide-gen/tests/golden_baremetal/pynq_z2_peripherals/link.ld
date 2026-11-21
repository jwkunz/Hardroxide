/* Linker script template for a program on the Zynq-7000 Cortex-A9 (hardroxide-ps7).
   Names between at-signs are placeholders the generator fills in. */
ENTRY(_start)

MEMORY
{
    RAM : ORIGIN = 0x00100000, LENGTH = 0x1ff00000
}

SECTIONS
{
    .text : ALIGN(32)
    {
        KEEP(*(.text._start))
        KEEP(*(.text.hrx_vectors))
        KEEP(*(.text.hrx_exceptions))
        *(.text .text.*)
    } > RAM

    .rodata : ALIGN(8) { *(.rodata .rodata.*) } > RAM
    .data   : ALIGN(8) { *(.data .data.*) } > RAM

    .bss (NOLOAD) : ALIGN(8)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
        __bss_end = .;
    } > RAM

    /* DMA buffers: placed together so the MMU can give them their own attributes. */
    .dma (NOLOAD) : ALIGN(4096)
    {
        __dma_start = .;
        *(.dma .dma.*)
        . = ALIGN(4096);
        __dma_end = .;
    } > RAM

    .stacks (NOLOAD) : ALIGN(8)
    {
        . += 0x400; . = ALIGN(8); __stack_fiq_top = .;
        . += 0x2000; . = ALIGN(8); __stack_irq_top = .;
        . += 0x400; . = ALIGN(8); __stack_abt_top = .;
        . += 0x400; . = ALIGN(8); __stack_und_top = .;
        . += 0x1000; . = ALIGN(8); __stack_svc_top = .;
        . += 0x10000; . = ALIGN(8); __stack_sys_top = .;
    } > RAM

    /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment) *(.note*) }
}
