// Storage code generation modules — one per data type family.
//
// Each module generates the C source and header for its storage type,
// including perfect hash tables, accessor functions, and initialization.

pub mod boolean;
pub mod integer;
pub mod persistence;
pub mod string;
