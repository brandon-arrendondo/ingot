Examples
========

Three example models are included in ``examples/``:

===============  =====  =========  ======  =====================================================
Model            Keys   Classes    Enums   Features
===============  =====  =========  ======  =====================================================
``minimal.toml`` 5      2          1       Bool, integers, string, persistence, helpers
``battery.toml`` 18     2          2       Per-variant defaults, enums with variants, read-only strings, events
``full.toml``    38     4          3       All types, all flags, comprehensive coverage
===============  =====  =========  ======  =====================================================

Pre-generated C output for each model is in ``examples/generated/``.
