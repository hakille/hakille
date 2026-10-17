use annora_core::{AnnoraError, NodeId, Result, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HouseholdRole {
    Owner,
    Adult,
    Minor,
    Guest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentFilterLevel {
    None,
    Moderate,
    Strict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Restriction {
    NoWallet,
    ContentFilter(ContentFilterLevel),
    SupervisedCategories(Vec<String>),
    TimeLimits { daily_minutes: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HouseholdMember {
    pub user_id: UserId,
    pub display_name: String,
    pub role: HouseholdRole,
    pub restrictions: Vec<Restriction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Household {
    pub members: HashMap<UserId, HouseholdMember>,
    pub shared_graph_id: NodeId,
}

impl Household {
    pub fn new(shared_graph_id: NodeId) -> Self {
        Self {
            members: HashMap::new(),
            shared_graph_id,
        }
    }

    pub fn add_member(&mut self, caller: &UserId, member: HouseholdMember) -> Result<()> {
        self.enforce_owner(caller)?;
        self.members.insert(member.user_id.clone(), member);
        Ok(())
    }

    pub fn remove_member(&mut self, caller: &UserId, user_id: &UserId) -> Result<()> {
        self.enforce_owner(caller)?;
        self.members.remove(user_id);
        Ok(())
    }

    /// Garde : seul un `Owner` peut ajouter / retirer des membres.
    /// Fail-closed si `caller` inconnu ou non-Owner.
    pub fn enforce_owner(&self, caller: &UserId) -> Result<()> {
        if self.is_owner(caller) {
            Ok(())
        } else {
            Err(AnnoraError::CapabilityDenied(
                "Seul le Owner du foyer peut ajouter/retirer un membre".to_string(),
            ))
        }
    }

    pub fn get_member(&self, user_id: &UserId) -> Option<&HouseholdMember> {
        self.members.get(user_id)
    }

    pub fn is_owner(&self, user_id: &UserId) -> bool {
        self.members
            .get(user_id)
            .map(|m| m.role == HouseholdRole::Owner)
            .unwrap_or(false)
    }

    pub fn get_restrictions(&self, user_id: &UserId) -> Vec<Restriction> {
        self.members
            .get(user_id)
            .map(|m| m.restrictions.clone())
            .unwrap_or_default()
    }
}
