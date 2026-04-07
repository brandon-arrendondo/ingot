32-Bit Key Encoding
===================

Each key is encoded as a 32-bit unsigned integer with the following bit layout:

::

    | Namespace | Class   | ID      | Type    | Thread  | Derived | Read    |
    | (10 bits) | (5 bits)| (10 bit)| (4 bit) | Safe(1) | (1 bit) | Only(1) |
    | 31-22     | 21-17   | 16-7    | 6-3     | 2       | 1       | 0       |

This encoding allows up to 1024 namespaces, 32 classes per namespace, and
1024 keys per type per class. The type field supports 16 data types. Key
properties (read-only, thread-safe) are encoded directly in the key value
for O(1) property checks.
