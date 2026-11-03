CLI Reference
=============

::

    ingot -m <PATH>... [OPTIONS]

Required
--------

====================================  ==========================================
Option                                Description
====================================  ==========================================
``-m, --model <PATH>``                Path to TOML data model file(s) or
                                      directory of TOML files. Repeatable —
                                      multiple ``-m`` flags merge all inputs
                                      into a single data model.
====================================  ==========================================

Output & Target
---------------

+------------------------+----------------+--------------------------------------------------+
| Option                 | Default        | Description                                      |
+========================+================+==================================================+
| ``-o, --output <DIR>`` | ``generated/`` | Output directory for generated code              |
+------------------------+----------------+--------------------------------------------------+
| ``-t, --target <T>``   | ``linux64``    | Target platform (see below)                      |
+------------------------+----------------+--------------------------------------------------+
| ``--no-events``        | off            | Disable event callback generation                |
+------------------------+----------------+--------------------------------------------------+
| ``--emit-tinyfsm``     | off            | Emit C++/tinyfsm event types and dispatch        |
+------------------------+----------------+--------------------------------------------------+
| ``--templates <DIR>``  | auto           | Override template directory                      |
+------------------------+----------------+--------------------------------------------------+

``--templates`` also accepts the ``INGOT_TEMPLATES_DIR`` environment variable.
``--emit-tinyfsm`` applies only to C targets. It emits additive C++ headers and
source for consumers using tinyfsm.

Key Filtering
-------------

=====================================  ==========================================
Option                                 Description
=====================================  ==========================================
``--include-list <FILE>``              YAML list of keys to include (whitelist);
                                       all others are excluded
``--exclude-list <FILE>``              YAML list of keys to exclude (blacklist);
                                       all others are included
``--persistent-keys <FILE>``           YAML list of keys that should be marked
                                       persistent
``--property-override-list <FILE>``    YAML map of per-key property overrides
                                       (``KEY: {default_value: val}``)
=====================================  ==========================================

``--include-list`` and ``--exclude-list`` are mutually exclusive.

Key names in filter lists are matched with underscore-normalized comparison,
so both ``STATEOFCHARGE`` and ``STATE_OF_CHARGE`` will match the same key.

Variant Overrides
-----------------

=========================  ==========================================
Option                     Description
=========================  ==========================================
``--variant <NAME>``       Product variant for per-variant default
                           overrides (e.g. ``peabodyv0``,
                           ``omnidrive_v2``). Applies variant-specific
                           defaults from ``[classes.keys.defaults]``
                           and merges variant enum values from
                           ``[enums.*.variants.*]``.
=========================  ==========================================

Verbosity
---------

+-----------------------------+---------+----------------------------------+
| Option                      | Default | Description                      |
+=============================+=========+==================================+
| ``-v`` / ``-vv`` / ``-vvv`` | warn    | Verbosity: info / debug / trace  |
+-----------------------------+---------+----------------------------------+


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
``rust``         no_std Rust module  Caller-managed        Generates ``dm.rs``
===============  ==================  ====================  ==============================

Thread-safe keys are protected by the target's mutex mechanism. Keys without
``thread_safe = true`` bypass locking entirely.

The ``rust`` target generates a standalone no_std Rust module using static
perfect-hash storage. It provides named accessors, scalar access by encoded
key, and ``PERSISTENT_KEYS`` metadata. Persistence I/O and mutex handling are
left to the consumer. C-only options such as ``--no-events`` and
``--emit-tinyfsm`` have no effect for Rust.


Examples
--------

Generate from a single model file::

    ingot -m device.toml -o src/dm/ -t stm32

Merge a directory of namespace TOML files::

    ingot -m models/ -o gen/udm -t linux64

Merge files from multiple locations::

    ingot -m common/base.toml -m project/overrides.toml -o gen/

Use include list and persistent keys::

    ingot -m models/ --include-list conf/include.yml \
        --persistent-keys conf/persist.yml -o gen/

Apply per-key property overrides::

    ingot -m models/ --property-override-list conf/overrides.yml -o gen/

Generate for a specific product variant::

    ingot -m models/ --variant peabodyv0 -o gen/
