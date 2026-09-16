use crate::models::MockOptions;
use anyhow::Result;
use fake::Fake;
use rand::RngExt;

/// Generate a random company name
pub fn generate_company(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, company::CompanyName))
}

/// Generate a random product name
pub fn generate_product(_options: &MockOptions) -> Result<String> {
    let adjectives = [
        "Ultra",
        "Premium",
        "Professional",
        "Advanced",
        "Deluxe",
        "Elite",
        "Pro",
        "Smart",
        "Digital",
        "Wireless",
        "Portable",
        "Compact",
        "Ergonomic",
        "High-Performance",
    ];
    let nouns = [
        "Widget",
        "Device",
        "Tool",
        "System",
        "Solution",
        "Platform",
        "Interface",
        "Controller",
        "Monitor",
        "Analyzer",
        "Generator",
        "Processor",
        "Manager",
        "Assistant",
    ];

    let adjective = adjectives[rand::rng().random_range(0..adjectives.len())];
    let noun = nouns[rand::rng().random_range(0..nouns.len())];

    Ok(format!("{adjective} {noun}"))
}

/// Generate a random product description of at most `options.length` bytes
/// (default 100).
///
/// Whole sentences are appended while they fit. When even the first sentence
/// does not fit, it is cut at the last whole word inside the limit, or
/// mid-word for a limit shorter than the first word, so any nonzero `length`
/// yields a non-empty description. A `length` of zero yields an empty string.
pub fn generate_product_description(options: &MockOptions) -> Result<String> {
    let length = options.length.unwrap_or(100);
    if length == 0 {
        return Ok(String::new());
    }

    let features = [
        "cutting-edge technology",
        "user-friendly interface",
        "robust performance",
        "seamless integration",
        "advanced analytics",
        "real-time monitoring",
        "cloud-based architecture",
        "enterprise-grade security",
        "scalable design",
        "intuitive workflow",
        "automated processes",
        "data-driven insights",
        "cross-platform compatibility",
        "high reliability",
        "cost-effective solution",
    ];

    let benefits = [
        "improves productivity",
        "reduces costs",
        "enhances efficiency",
        "streamlines operations",
        "accelerates growth",
        "minimizes risks",
        "maximizes ROI",
        "delivers value",
        "ensures compliance",
        "optimizes performance",
        "simplifies management",
        "increases revenue",
    ];

    let verbs = [
        "designed to",
        "built to",
        "engineered to",
        "created to",
        "developed to",
    ];

    let mut description = String::new();

    loop {
        let verb = verbs[rand::rng().random_range(0..verbs.len())];
        let benefit = benefits[rand::rng().random_range(0..benefits.len())];
        let feature = features[rand::rng().random_range(0..features.len())];

        let sentence = format!("This product is {verb} {benefit} through {feature}. ");

        if description.len() + sentence.len() > length {
            if description.is_empty() {
                description = truncate_at_word_boundary(&sentence, length);
            }
            break;
        }

        description.push_str(&sentence);
    }

    Ok(description.trim_end().to_string())
}

/// Cuts `sentence` to at most `length` bytes, preferring the last whole word
/// inside the limit and falling back to a mid-word (char-boundary) cut when the
/// limit is shorter than the first word.
fn truncate_at_word_boundary(sentence: &str, length: usize) -> String {
    if sentence.len() <= length {
        return sentence.to_string();
    }

    let mut cut = length;
    while !sentence.is_char_boundary(cut) {
        cut -= 1;
    }

    let head = &sentence[..cut];
    match head.rfind(' ') {
        Some(space) if space > 0 => head[..space].to_string(),
        _ => head.to_string(),
    }
}

/// Generate a random job title
pub fn generate_job_title(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, job::Title))
}

/// Generate a random industry name
pub fn generate_industry(options: &MockOptions) -> Result<String> {
    Ok(localized!(options.locale, company::Industry))
}

/// Generate a random business buzzword
pub fn generate_buzzword(_options: &MockOptions) -> Result<String> {
    let buzzwords = [
        "Synergy",
        "Paradigm",
        "Leverage",
        "Optimization",
        "Innovation",
        "Disruption",
        "Transformation",
        "Scalability",
        "Agility",
        "Efficiency",
        "Integration",
        "Collaboration",
        "Streamlining",
        "Automation",
        "Analytics",
        "Intelligence",
        "Convergence",
        "Sustainability",
        "Monetization",
        "Engagement",
        "Empowerment",
        "Digitization",
        "Personalization",
        "Gamification",
        "Blockchain",
        "AI-driven",
        "Cloud-native",
        "Omnichannel",
        "Customer-centric",
        "Data-driven",
        "Results-oriented",
    ];

    let buzzword = buzzwords[rand::rng().random_range(0..buzzwords.len())];
    Ok(buzzword.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DataType, Locale};
    use rstest::rstest;

    fn options() -> MockOptions {
        MockOptions {
            data_type: DataType::Product,
            min: None,
            max: None,
            length: None,
            precision: None,
            age: None,
            past: false,
            future: false,
            range: None,
            locale: Locale::En,
        }
    }

    #[rstest]
    #[case::company(generate_company)]
    #[case::job_title(generate_job_title)]
    #[case::industry(generate_industry)]
    #[case::buzzword(generate_buzzword)]
    fn generator_output_is_non_empty(#[case] generator: fn(&MockOptions) -> Result<String>) {
        assert!(!generator(&options()).unwrap().is_empty());
    }

    #[test]
    fn generate_product_has_two_non_empty_words() {
        let product = generate_product(&options()).unwrap();
        let words: Vec<&str> = product.split(' ').collect();
        assert_eq!(words.len(), 2);
        assert!(words.iter().all(|w| !w.is_empty()));
    }

    #[test]
    fn product_description_zero_length_is_empty() {
        let mut opts = options();
        opts.length = Some(0);

        assert_eq!(generate_product_description(&opts).unwrap(), "");
    }

    // The sentence template spans roughly 64 to 91 bytes, so the cases below
    // cover a mid-word cut (3), a word-boundary cut (10, 50), the band where a
    // sentence only sometimes fits (70), and multi-sentence output (200).
    #[rstest]
    #[case::shorter_than_first_word(3)]
    #[case::tiny(10)]
    #[case::below_one_sentence(50)]
    #[case::sometimes_one_sentence(70)]
    #[case::several_sentences(200)]
    fn product_description_is_non_empty_and_respects_length(#[case] length: usize) {
        let mut opts = options();
        opts.length = Some(length);

        let description = generate_product_description(&opts).unwrap();

        assert!(!description.is_empty());
        assert!(description.len() <= length);
    }

    #[rstest]
    #[case::mid_word_cut("This product is designed to", 6, "This")]
    #[case::shorter_than_first_word("This product", 3, "Thi")]
    #[case::exact_fit("Short one.", 100, "Short one.")]
    fn truncate_at_word_boundary_cases(
        #[case] sentence: &str,
        #[case] length: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(truncate_at_word_boundary(sentence, length), expected);
    }
}
