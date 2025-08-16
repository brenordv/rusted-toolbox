use crate::models::MockOptions;
use anyhow::Result;
use fake::faker::company::en::*;
use fake::faker::job::en::*;
use fake::Fake;
use rand::Rng;

/// Generate a random company name
pub fn generate_company(_options: &MockOptions) -> Result<String> {
    Ok(CompanyName().fake::<String>())
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

    let adjective = adjectives[rand::thread_rng().gen_range(0..adjectives.len())];
    let noun = nouns[rand::thread_rng().gen_range(0..nouns.len())];

    Ok(format!("{} {}", adjective, noun))
}

/// Generate a random product description
pub fn generate_product_description(options: &MockOptions) -> Result<String> {
    let length = options.length.unwrap_or(100);

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
    let mut current_length = 0;

    while current_length < length {
        let verb = verbs[rand::thread_rng().gen_range(0..verbs.len())];
        let benefit = benefits[rand::thread_rng().gen_range(0..benefits.len())];
        let feature = features[rand::thread_rng().gen_range(0..features.len())];

        let sentence = format!("This product is {} {} through {}. ", verb, benefit, feature);

        if current_length + sentence.len() > length {
            break;
        }

        description.push_str(&sentence);
        current_length += sentence.len();
    }

    // Trim to desired length if necessary
    if description.len() > length {
        description.truncate(length);
        // Try to end at a word boundary
        if let Some(last_space) = description.rfind(' ') {
            description.truncate(last_space);
        }
        description.push('.');
    }

    Ok(description.trim().to_string())
}

/// Generate a random job title
pub fn generate_job_title(_options: &MockOptions) -> Result<String> {
    Ok(Title().fake::<String>())
}

/// Generate a random industry name
pub fn generate_industry(_options: &MockOptions) -> Result<String> {
    Ok(Industry().fake::<String>())
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

    let buzzword = buzzwords[rand::thread_rng().gen_range(0..buzzwords.len())];
    Ok(buzzword.to_string())
}
