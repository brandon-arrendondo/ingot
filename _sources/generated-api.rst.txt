Generated API
=============


Initialization
--------------

.. code-block:: c

    #include "dm.h"

    // With event callbacks:
    void my_callback(uint32_t key) { /* handle change */ }
    DataModel_Initialize(my_callback);

    // Without events (--no-events):
    DataModel_Initialize();

    // Cleanup:
    DataModel_TearDown();


Reading and Writing Values
--------------------------

All integral types (bool, integers) use a type-dispatch API:

.. code-block:: c

    #include "dm.h"
    #include "key_definitions.h"

    // Set a value
    dm_val_t val = { 0 };
    val.u16val = 42;
    DM_RETURN_CODE rc = DataModel_SetIntegralTypeByKey(DM_KEY_..., val);

    // Get a value
    dm_val_t out = DataModel_GetIntegralTypeByKey(DM_KEY_...);
    uint16_t temp = out.u16val;

Typed convenience setters are also generated:

.. code-block:: c

    DataModel_SetBooleanByKey(DM_KEY_..., true);
    DataModel_SetUInt16ByKey(DM_KEY_..., 42);
    DataModel_SetInt32ByKey(DM_KEY_..., -7);


String API
----------

.. code-block:: c

    // Get
    const char *str = NULL;
    DM_RETURN_CODE rc = DataModel_GetStringByKey(DM_KEY_..., &str);

    // Set (read-write strings only)
    rc = DataModel_SetStringByKey(DM_KEY_..., "new value");

String sets are bounds-checked against ``max_size``. Strings exceeding the limit
are rejected.


Helper Functions
----------------

Keys with ``helpers = true`` get named getter/setter functions:

.. code-block:: c

    #include "dm_helpers.h"

    // Integral types: static inline in the header
    uint16_t temp = DataModel_Get_MY_DEVICE_STATUS_TEMPERATURE();
    DataModel_Set_MY_DEVICE_STATUS_TEMPERATURE(2950);

    // String types: declarations in header, implementations in dm_helpers.c
    const char *name = DataModel_Get_MY_DEVICE_CONFIG_DEVICE_NAME();
    DataModel_Set_MY_DEVICE_CONFIG_DEVICE_NAME("new-name");


Return Codes
------------

.. code-block:: c

    typedef enum {
        DM_RETURN_CODE_SUCCESS              =  1,
        DM_RETURN_CODE_NOT_INITIALIZED      = -1,
        DM_RETURN_CODE_KEY_TYPE_INCORRECT   = -2,
        DM_RETURN_CODE_SET_ON_READONLY_KEY  = -3,
        DM_RETURN_CODE_STORAGE_LOOKUP_FAILURE = -4,
        DM_RETURN_CODE_SET_VALUE_UNCHANGED  = -5,
        DM_RETURN_CODE_NO_KEYS_OF_TYPE      = -6,
        DM_RETURN_CODE_OUTPUT_VARIABLE_NULL = -7,
        // When persistence is enabled:
        DM_RETURN_CODE_PERSISTENCE_FILE_NOT_FOUND  = -8,
        DM_RETURN_CODE_PERSISTENCE_READ_FAILURE    = -9,
        DM_RETURN_CODE_PERSISTENCE_WRITE_FAILURE   = -10,
        DM_RETURN_CODE_PERSISTENCE_MAGIC_MISMATCH  = -11,
    } DM_RETURN_CODE;


Key Query Macros
----------------

The generated ``dm_key.h`` provides macros to inspect key properties at runtime:

.. code-block:: c

    #include "dm_key.h"

    DM_IS_KEY_READONLY(key)       // Is this key read-only?
    DM_IS_KEY_THREADSAFE(key)     // Does this key require mutex protection?
    DM_IS_KEY_DATA_TYPE(key, t)   // Does this key have data type t?
    DM_IS_KEY_IN_NAMESPACE(key, n)// Is this key in namespace n?
    DM_KEY_GET_TYPE(key)          // Extract the 4-bit type code


Persistence
-----------

Keys marked ``persistent = true`` can be saved to and loaded from a binary file:

.. code-block:: c

    #include "persistence_storage.h"

    // Save all persistent key values to a file
    DM_RETURN_CODE rc = DataModel_SavePersistentKeys("/data/dm.bin");

    // Load and restore persistent key values from a file
    rc = DataModel_LoadPersistentKeys("/data/dm.bin");

    // Pass NULL to use the default path ("/sdcard/dm.bin")
    rc = DataModel_LoadPersistentKeys(NULL);

    // Check if a specific key is persistent
    bool is_persistent = PersistenceStorage_IsKeyPersistent(DM_KEY_...);

The binary format uses a packed struct with a magic number
(``sizeof(struct) XOR num_keys``) to detect layout changes. If the magic number
does not match on load, ``DM_RETURN_CODE_PERSISTENCE_MAGIC_MISMATCH`` is
returned and the live storage is not modified.
