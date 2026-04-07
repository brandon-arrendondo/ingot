CLI Reference
=============

::

    ingot --model <PATH> [--output <DIR>] [--target <TARGET>] [--no-events] [-v]

======================  ============  ==========================================
Option                  Default       Description
======================  ============  ==========================================
``--model <PATH>``      (required)    Path to TOML data model file
``--output <DIR>``      ``generated/``  Output directory for generated C code
``--target <TARGET>``   ``linux64``   Target platform (see below)
``--no-events``         off           Disable event callback generation
``-v`` / ``-vv`` / ``-vvv``  warn     Verbosity: info / debug / trace
======================  ============  ==========================================


Targets
-------

===============  ==================  ====================  ==============================
Value            Platform            Mutex                 Notes
===============  ==================  ====================  ==============================
``stm32``        32-bit ARM STM32    None (bare-metal)     4-byte alignment
``esp-xtensa``   ESP32 Xtensa        FreeRTOS semaphore    ``xSemaphoreTake``/``Give``
``esp-riscv``    ESP32 RISC-V        FreeRTOS semaphore    ``xSemaphoreTake``/``Give``
``mcu8bit``      8-bit MCU           None (bare-metal)     1-byte alignment, 16-bit pointers
``linux64``      64-bit Linux        pthread mutex         ``PTHREAD_MUTEX_INITIALIZER``
===============  ==================  ====================  ==============================

Thread-safe keys are protected by the target's mutex mechanism. Keys without
``thread_safe = true`` bypass locking entirely.
