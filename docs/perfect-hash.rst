Perfect Hash Algorithm
======================

Ingot uses the CHM (Czech-Havas-Majewski) algorithm with a 2-seed Jenkins
lookup3 ``final()`` hash function. For each group of keys (by type), it:

1. Picks two random 32-bit seeds
2. Builds an undirected graph where edges map to keys
3. Verifies the graph is acyclic (retries with new seeds if not)
4. Assigns G-table values so ``(G[h1(key)] + G[h2(key)]) % n`` gives a unique
   index for each key

The G-table is typically ~2x the number of keys. Values are stored in the
smallest signed integer type that fits (``int8_t``, ``int16_t``, or ``int32_t``).
