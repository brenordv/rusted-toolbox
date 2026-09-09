//! Generator functions grouped by category. Options reaching these functions
//! are assumed pre-validated by `generate_mock_data` (numeric bounds, date
//! ranges); calling a generator directly with out-of-range options (for
//! example `min > max`) can panic inside the random-range sampling.

pub mod commerce;
pub mod internet;
pub mod personal;
pub mod random;

pub use commerce::*;
pub use internet::*;
pub use personal::*;
pub use random::*;
