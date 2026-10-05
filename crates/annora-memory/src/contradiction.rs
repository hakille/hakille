use crate::triplet::Triplet;

pub enum ContradictionType {
    DirectConflict,
    TemporalConflict,
    ConfidenceConflict,
}

pub enum Resolution {
    KeepExisting,
    ReplaceWithNew,
    AskUser { reason: String },
}

pub fn detect_contradiction(_existing: &Triplet, _incoming: &Triplet) -> Option<ContradictionType> {
    None
}

pub fn resolve(_existing: &Triplet, _incoming: &Triplet) -> Resolution {
    Resolution::KeepExisting
}
