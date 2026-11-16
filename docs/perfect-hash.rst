Perfect Hash Algorithm
======================

Ingot uses the CHM (Czech-Havas-Majewski) algorithm with a 2-seed Jenkins
lookup3 ``final()`` hash function. For each group of keys (by type), it:

1. Derives two 32-bit seeds from the FNV-1a hash of the key set, so the same
   keys always give the same tables
2. Builds an undirected graph where edges map to keys
3. Verifies the graph is acyclic (if not, derives the next seed pair
   deterministically and retries)
4. Assigns G-table values so ``(G[h1(key)] + G[h2(key)]) % n`` gives a unique
   index for each key

The G-table is typically ~2x the number of keys. Values are stored in the
smallest signed integer type that fits (``int8_t``, ``int16_t``, or ``int32_t``).
