use crate::household::{ContentFilterLevel, HouseholdMember, Restriction};
use annora_core::UserId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgeCategory {
    Child,
    Teenager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinorProfile {
    pub member: HouseholdMember,
    pub parent_user_ids: Vec<UserId>,
    pub age_category: AgeCategory,
}

pub fn default_minor_restrictions(category: &AgeCategory) -> Vec<Restriction> {
    let mut restrictions = vec![Restriction::NoWallet];

    match category {
        AgeCategory::Child => {
            restrictions.push(Restriction::ContentFilter(ContentFilterLevel::Strict));
            restrictions.push(Restriction::SupervisedCategories(vec!["all".to_string()]));
        }
        AgeCategory::Teenager => {
            restrictions.push(Restriction::ContentFilter(ContentFilterLevel::Moderate));
            restrictions.push(Restriction::SupervisedCategories(vec![
                "finance".to_string(),
                "mature_content".to_string(),
            ]));
        }
    }

    restrictions
}
