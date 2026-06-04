Testing Generated Code
======================

Ingot generates a Unity test suite (``test_dm.c``) that validates:

- **Default values**: every key returns its declared default after init
- **Set/get roundtrip**: set a test value, read it back, verify equality
- **Read-only rejection**: SetIntegralTypeByKey returns error for read-only keys
- **Unchanged detection**: setting the same value returns SET_VALUE_UNCHANGED
- **String roundtrip**: set and get for read-write string keys
- **Persistence roundtrip**: save, reset, load, verify (when persistent keys exist)

To run the generated tests:

::

    # Initialize Unity submodule (first time)
    git submodule update --init deps/unity

    # Build and run
    cmake -S generated/ -B build/ -DUNITY_DIR=deps/unity/src
    cmake --build build/
    ./build/test_dm

Or use the invoke task which runs all models in both events/no-events modes:

::

    invoke test
