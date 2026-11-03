==================
Ingot User Guide
==================

Ingot is a code generator for embedded key-value databases. It reads a TOML
data model specification and produces optimized C99 for supported platform
targets or a ``no_std`` Rust module. Both use compile-time perfect hashing for
O(1) key lookup and statically allocated storage.

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
