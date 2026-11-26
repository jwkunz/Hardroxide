/* Linker script template for a program on the Zynq UltraScale+ Cortex-A53 (hardroxide-psu).
   Names between at-signs are placeholders the generator fills in. RAM must be in DDR below
   2 GiB. */
ENTRY(_start)

MEMORY
{
    RAM : ORIGIN = 0x00100000, LENGTH = 0x7ff00000
}

SECTIONS
{
    .text : ALIGN(2048)
    {
        KEEP(*(.text._start))
        . = ALIGN(2048);
        KEEP(*(.text.hrx_vectors))
        KEEP(*(.text.hrx_exceptions))
        *(.text .text.*)
    } > RAM

    .rodata : ALIGN(16) { *(.rodata .rodata.*) } > RAM
    .data   : ALIGN(16) { *(.data .data.*) *(.got .got.*) } > RAM

    .bss (NOLOAD) : ALIGN(16)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(16);
        __bss_end = .;
    } > RAM

    /* DMA buffers: placed together, on whole 2 MiB blocks, so the MMU can make them
       non-cacheable (see hardroxide_psu::mem). */
    .dma (NOLOAD) : ALIGN(0x200000)
    {
        __dma_start = .;
        *(.dma .dma.*)
        . = ALIGN(0x200000);
        __dma_end = .;
    } > RAM

    .stacks (NOLOAD) : ALIGN(16)
    {
        . += 0x4000; . = ALIGN(16); __stack_exc_top = .;
        . += 0x10000; . = ALIGN(16); __stack_main_top = .;
    } > RAM

    /DISCARD/ : { *(.eh_frame*) *(.comment) *(.note*) }
}
