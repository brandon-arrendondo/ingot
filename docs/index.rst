==================
Ingot User Guide
==================

Ingot is a C code generator for embedded key-value databases. It reads a TOML
data model specification and produces optimized C99 code with compile-time
perfect hashing for O(1) key lookup. All storage is statically allocated --
no heap, no dynamic memory, no fragmentation.

.. toctree::
   :maxdepth: 3
   :caption: Contents

   installation
   quick-start
   cli-reference
   schema-reference
   generated-api
   build-integration
   testing
   examples
   key-encoding
   perfect-hash
