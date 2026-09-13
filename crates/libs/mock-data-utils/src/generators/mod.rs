//! Generator functions grouped by category. Options reaching these functions
//! are assumed pre-validated by `generate_mock_data` (numeric bounds, date
//! ranges); calling a generator directly with out-of-range options (for
//! example `min > max`) can panic inside the random-range sampling.

/// Runs a `fake` faker under the given [`crate::models::Locale`], returning a
/// `String`. Takes the faker's module and name (`localized!(locale,
/// name::FirstName)`) and dispatches to the matching locale submodule; every
/// arm completes the call with `.fake::<String>()` because each locale
/// submodule returns a differently-typed faker (`raw::Faker<LOCALE>`), so the
/// bare values could not share a match. Callers need `fake::Fake` in scope.
/// Defined before the submodules, so macro_rules! textual scoping puts it in
/// scope for all of them without imports.
macro_rules! localized {
    ($locale:expr, $module:ident :: $faker:ident) => {
        match $locale {
            $crate::models::Locale::En => fake::faker::$module::en::$faker().fake::<String>(),
            $crate::models::Locale::ArSa => fake::faker::$module::ar_sa::$faker().fake::<String>(),
            $crate::models::Locale::CyGb => fake::faker::$module::cy_gb::$faker().fake::<String>(),
            $crate::models::Locale::DeDe => fake::faker::$module::de_de::$faker().fake::<String>(),
            $crate::models::Locale::FaIr => fake::faker::$module::fa_ir::$faker().fake::<String>(),
            $crate::models::Locale::FrFr => fake::faker::$module::fr_fr::$faker().fake::<String>(),
            $crate::models::Locale::ItIt => fake::faker::$module::it_it::$faker().fake::<String>(),
            $crate::models::Locale::JaJp => fake::faker::$module::ja_jp::$faker().fake::<String>(),
            $crate::models::Locale::NlNl => fake::faker::$module::nl_nl::$faker().fake::<String>(),
            $crate::models::Locale::PtBr => fake::faker::$module::pt_br::$faker().fake::<String>(),
            $crate::models::Locale::PtPt => fake::faker::$module::pt_pt::$faker().fake::<String>(),
            $crate::models::Locale::TrTr => fake::faker::$module::tr_tr::$faker().fake::<String>(),
            $crate::models::Locale::ZhCn => fake::faker::$module::zh_cn::$faker().fake::<String>(),
            $crate::models::Locale::ZhTw => fake::faker::$module::zh_tw::$faker().fake::<String>(),
        }
    };
}

pub mod commerce;
pub mod internet;
pub mod personal;
pub mod random;

pub use commerce::*;
pub use internet::*;
pub use personal::*;
pub use random::*;
