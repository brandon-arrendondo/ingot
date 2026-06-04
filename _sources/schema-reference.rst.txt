Schema Reference
================

A data model file has three top-level sections: ``[meta]``, ``[enums]``, and
``[[classes]]``.


Meta
----

.. code-block:: toml

    [meta]
    id = "my_device"        # Namespace identifier (required)
    version = "1.0.0"       # Semantic version (required)
    doc = "Description"     # Optional documentation

The ``id`` is used to prefix all generated ``#define`` names:
``DM_KEY_{ID}_{CLASS}_{KEY}``.


Enums
-----

Enums define named integer-to-label mappings reusable across keys.

.. code-block:: toml

    [enums.mode]
    doc = "Operating mode"

    [enums.mode.values]
    off = 0
    idle = 1
    active = 2

    # Optional: per-variant overrides
    [enums.mode.variants.compact]
    off = 0
    active = 1

Reference an enum on a key with ``enum = "mode"``. The enum values appear as
comments in the generated ``key_definitions.h``.


Classes
-------

Classes group related keys within a namespace. A namespace can have up to 31
classes, each with up to 1023 keys.

.. code-block:: toml

    [[classes]]
    id = "status"
    doc = "Runtime status"

        [[classes.keys]]
        id = "temperature"
        type = "uint16"
        # ... attributes


Key Attributes
--------------

===============  ========  ===========  ==========================================
Attribute        Type      Default      Description
===============  ========  ===========  ==========================================
``id``           string    (required)   Key identifier
``type``         string    (required)   Data type (see below)
``default``      value     type zero    Default value
``defaults``     table     ``{}``       Per-variant default overrides
``enum``         string    none         Reference to an ``[enums]`` entry
``max_size``     integer   none         Max bytes (required for string/binary)
``read_only``    bool      ``false``    Prevent modification at runtime
``thread_safe``  bool      ``false``    Protect with target mutex
``persistent``   bool      ``false``    Include in binary save/load
``event``        bool      ``false``    Fire callback on value change
``helpers``      bool      ``false``    Generate named getter/setter functions
``unit``         string    none         Physical unit (documentary)
``doc``          string    none         Documentation string
===============  ========  ===========  ==========================================


Data Types
----------

============  ==============  =========  =======================================
Type          C Type          Size       Notes
============  ==============  =========  =======================================
``bool``      ``bool``        1 bit      Packed into uint32_t bitfields
``uint8``     ``uint8_t``     1 byte
``int8``      ``int8_t``      1 byte
``uint16``    ``uint16_t``    2 bytes
``int16``     ``int16_t``     2 bytes
``uint32``    ``uint32_t``    4 bytes
``int32``     ``int32_t``     4 bytes
``string``    ``char[]``      max_size   Requires ``max_size`` attribute
``binary``    ``uint8_t[]``   max_size   Requires ``max_size`` (schema only, codegen pending)
============  ==============  =========  =======================================


Per-Variant Defaults
--------------------

Keys can have different default values for different product variants:

.. code-block:: toml

    [[classes.keys]]
    id = "threshold"
    type = "uint8"
    default = 10              # Base default

    [classes.keys.defaults]
    compact = 5               # Override for "compact" variant
    pro = 15                  # Override for "pro" variant

The base ``default`` applies when no variant-specific override exists.


Constraints
-----------

- ``string`` and ``binary`` types must specify ``max_size``
- ``read_only`` keys cannot be ``persistent`` (validated at parse time)
- ``enum`` references must point to a defined ``[enums]`` entry
- Class IDs must be unique within a namespace
- Key IDs must be unique within a class
