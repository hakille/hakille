pub mod context_partition;
pub mod contradiction;
pub mod decay;
pub mod export;
pub mod graph;
pub mod heritage;
pub mod objective;
pub mod query_lang;
pub mod storage;
pub mod task;
pub mod triplet;
pub mod vector;

pub use context_partition::*;
pub use contradiction::*;
pub use decay::*;
pub use export::*;
pub use graph::*;
pub use heritage::*;
pub use objective::*;
pub use query_lang::*;
pub use task::*;
pub use triplet::*;

#[cfg(test)]
mod tests {
    use super::*;
    use annora_core::{NodeId, PredicateId, Timestamp, TripletId, TrustLevel};

    #[test]
    fn test_task_lifecycle_and_bounded_steps() {
        let mut task = Task::new("task-42", "Envoyer compte-rendu", "Envoyer par email après réunion")
            .with_priority(TaskPriority::High)
            .with_parent_objective("obj-q3-marketing");

        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.progress_ratio(), 0.0);

        let s1 = task.add_step("Rédiger les notes", None);
        let s2 = task.add_step("Relire le plan d'action", None);
        let _s3 = task.add_step("Envoyer email", Some("capability:email-send".to_string()));

        assert_eq!(task.steps.len(), 3);

        task.complete_step(s1 as usize, Some("Notes rédigées".to_string()));
        assert_eq!(task.status, TaskStatus::InProgress);
        assert!((task.progress_ratio() - 0.333).abs() < 0.01);

        task.complete_step(s2 as usize, None);
        task.complete_step(2, Some("Email transmis".to_string()));
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.progress_ratio(), 1.0);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn test_declarative_graph_query_lang() {
        let mut graph = KnowledgeGraph::new();

        let subj_alice = NodeId(uuid::Uuid::new_v4());
        let pred_aime = PredicateId(uuid::Uuid::new_v4());

        let t1 = Triplet {
            id: TripletId(uuid::Uuid::new_v4()),
            subject: subj_alice.clone(),
            predicate: pred_aime.clone(),
            object: NodeValue::Text("Programmation en Rust".to_string()),
            metadata: TripletMetadata {
                weight: 0.95,
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 0.98,
                context: ContextPartition::Default,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
        };

        let t2 = Triplet {
            id: TripletId(uuid::Uuid::new_v4()),
            subject: subj_alice.clone(),
            predicate: pred_aime.clone(),
            object: NodeValue::Text("Café espresso".to_string()),
            metadata: TripletMetadata {
                weight: 0.40, // Lower weight
                created_at: Timestamp::now(),
                last_activated: Timestamp::now(),
                source: TripletSource::User,
                confidence: 0.85,
                context: ContextPartition::Personal,
                trust_level: TrustLevel::User,
                decay_rate: 0.01,
            },
        };

        graph.insert(t1);
        graph.insert(t2);

        // Query with text filter and weight cutoff
        let query = GraphQueryBuilder::select()
            .where_subject(subj_alice)
            .where_object_contains("Rust")
            .where_weight_above(0.5)
            .build();

        let results = query.execute(&graph);
        assert_eq!(results.len(), 1);
        match &results[0].object {
            NodeValue::Text(s) => assert!(s.contains("Rust")),
            _ => panic!("Expected text object"),
        }

        // Query by partition
        let query_personal = GraphQueryBuilder::select()
            .where_partition(ContextPartition::Personal)
            .build();
        let results_personal = query_personal.execute(&graph);
        assert_eq!(results_personal.len(), 1);
        match &results_personal[0].object {
            NodeValue::Text(s) => assert!(s.contains("Café")),
            _ => panic!("Expected café object"),
        }
    }
}
