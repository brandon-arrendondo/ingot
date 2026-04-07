Build System Integration
========================


CMake
-----

The generated ``CMakeLists.txt`` is for the Unity test suite. For your
application, add the generated ``.c`` files to your build:

.. code-block:: cmake

    add_library(data_model STATIC
        generated/jenkins_hash.c
        generated/boolean_storage.c
        generated/integer_storage.c
        generated/string_storage.c
        generated/persistence_storage.c  # if persistent keys exist
        generated/dm.c
        generated/dm_helpers.c           # if string helpers exist
    )
    target_include_directories(data_model PUBLIC generated/)


Make
----

.. code-block:: makefile

    DM_SRCS = $(wildcard generated/*.c)
    DM_OBJS = $(DM_SRCS:.c=.o)

    CFLAGS += -Igenerated/ -std=c99

    libdm.a: $(DM_OBJS)
    	$(AR) rcs $@ $^


ESP-IDF
-------

Create a component directory and symlink or copy the generated files:

::

    components/data_model/
        CMakeLists.txt
        include/          # symlink to generated headers
        src/              # symlink to generated .c files
