.syntax unified
.arm
.text
.global _start
.type _start, %function
_start:
    mov fp, #0
    mov lr, #0
    pop {r1}
    mov r2, sp
    push {r2}
    push {r0}
    mov r0, #0
    push {r0}
    mov r3, #0
    ldr r0, =main
    bl __libc_start_main
    udf #0
.size _start, .-_start
.section .note.GNU-stack,"",%progbits
