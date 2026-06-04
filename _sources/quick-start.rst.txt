Quick Start
===========

1. Write a data model in TOML:

.. code-block:: toml

    [meta]
    id = "my_device"
    version = "1.0.0"

    [[classes]]
    id = "config"

        [[classes.keys]]
        id = "brightness"
        type = "uint8"
        default = 100

2. Generate C code:

::

    ingot --model my_device.toml --output generated/

3. Include the generated files in your build system and use the API:

.. code-block:: c

    #include "dm.h"
    #include "key_definitions.h"

    DataModel_Initialize(NULL);

    dm_val_t val;
    val.u8val = 75;
    DataModel_SetIntegralTypeByKey(DM_KEY_MY_DEVICE_CONFIG_BRIGHTNESS, val);

    dm_val_t out = DataModel_GetIntegralTypeByKey(DM_KEY_MY_DEVICE_CONFIG_BRIGHTNESS);
    // out.u8val == 75
