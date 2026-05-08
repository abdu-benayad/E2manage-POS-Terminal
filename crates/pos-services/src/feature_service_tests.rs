//! Feature Service Tests (TDD)
//!
//! Tests for FeatureService - screen access checks and feature navigation.

#[cfg(test)]
mod tests {
    use crate::FeatureService;
    use pos_db::migrations::run_migrations;
    use pos_db::Database;
    use pos_models::{Feature, FeatureScreen};
    use std::sync::Arc;

    fn setup_test_service() -> (FeatureService, Arc<Database>) {
        let db = Database::in_memory().expect("Failed to create in-memory database");
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).expect("Failed to run migrations");
        }
        let db = Arc::new(db);
        let service = FeatureService::new(Arc::clone(&db));
        (service, db)
    }

    fn seed_features(db: &Database) {
        // Core feature (always enabled)
        db.upsert_feature(&Feature {
            feature_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_core: true,
            is_enabled: true,
            display_order: 1,
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "checkout".to_string(),
            screen_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_entry_point: true,
            display_order: 1,
            ..Default::default()
        })
        .unwrap();

        // Optional enabled feature with navigation chain
        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_core: false,
            is_enabled: true,
            display_order: 2,
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            is_entry_point: true,
            next_screen: Some("return-items".to_string()),
            display_order: 1,
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-items".to_string(),
            name: "Return Items".to_string(),
            is_entry_point: false,
            next_screen: Some("refund".to_string()),
            display_order: 2,
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "refund".to_string(),
            name: "Refund".to_string(),
            is_entry_point: false,
            next_screen: None,
            display_order: 3,
            ..Default::default()
        })
        .unwrap();

        // Disabled feature
        db.upsert_feature(&Feature {
            feature_id: "drafts".to_string(),
            name: "Drafts".to_string(),
            is_core: false,
            is_enabled: false,
            display_order: 3,
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "drafts".to_string(),
            screen_id: "save-draft".to_string(),
            name: "Save Draft".to_string(),
            is_entry_point: true,
            display_order: 1,
            ..Default::default()
        })
        .unwrap();
    }

    #[test]
    fn test_is_screen_enabled_for_enabled_feature() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        assert!(service.is_screen_enabled("checkout").unwrap());
        assert!(service.is_screen_enabled("return-entry").unwrap());
        assert!(service.is_screen_enabled("return-items").unwrap());
    }

    #[test]
    fn test_is_screen_disabled_for_disabled_feature() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        assert!(!service.is_screen_enabled("save-draft").unwrap());
    }

    #[test]
    fn test_is_screen_disabled_for_unknown_screen() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        assert!(!service.is_screen_enabled("unknown-screen").unwrap());
    }

    #[test]
    fn test_is_feature_enabled() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        assert!(service.is_feature_enabled("checkout").unwrap());
        assert!(service.is_feature_enabled("returns").unwrap());
        assert!(!service.is_feature_enabled("drafts").unwrap());
        assert!(!service.is_feature_enabled("unknown-feature").unwrap());
    }

    #[test]
    fn test_get_enabled_screen_ids() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        let screens = service.get_enabled_screen_ids().unwrap();

        assert!(screens.contains(&"checkout".to_string()));
        assert!(screens.contains(&"return-entry".to_string()));
        assert!(screens.contains(&"return-items".to_string()));
        assert!(screens.contains(&"refund".to_string()));
        assert!(!screens.contains(&"save-draft".to_string()));
    }

    #[test]
    fn test_get_next_screen() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        let next = service.get_next_screen("return-entry").unwrap();
        assert_eq!(next, Some("return-items".to_string()));

        let next = service.get_next_screen("return-items").unwrap();
        assert_eq!(next, Some("refund".to_string()));

        let next = service.get_next_screen("refund").unwrap();
        assert_eq!(next, None);

        let next = service.get_next_screen("checkout").unwrap();
        assert_eq!(next, None);
    }

    #[test]
    fn test_get_feature_entry_screen() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        let entry = service.get_feature_entry_screen("returns").unwrap();
        assert_eq!(entry, Some("return-entry".to_string()));

        let entry = service.get_feature_entry_screen("checkout").unwrap();
        assert_eq!(entry, Some("checkout".to_string()));

        let entry = service.get_feature_entry_screen("unknown").unwrap();
        assert_eq!(entry, None);
    }

    #[test]
    fn test_get_screen_feature() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        let feature = service.get_screen_feature("return-entry").unwrap();
        assert_eq!(feature, Some("returns".to_string()));

        let feature = service.get_screen_feature("checkout").unwrap();
        assert_eq!(feature, Some("checkout".to_string()));

        let feature = service.get_screen_feature("save-draft").unwrap();
        assert_eq!(feature, Some("drafts".to_string()));

        let feature = service.get_screen_feature("unknown").unwrap();
        assert_eq!(feature, None);
    }

    #[test]
    fn test_can_navigate_to_enabled_screen() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        assert!(service.can_navigate_to("checkout").unwrap());
        assert!(service.can_navigate_to("return-entry").unwrap());
        assert!(service.can_navigate_to("return-items").unwrap());
    }

    #[test]
    fn test_cannot_navigate_to_disabled_screen() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        assert!(!service.can_navigate_to("save-draft").unwrap());
    }

    #[test]
    fn test_cannot_navigate_to_unknown_screen() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        assert!(!service.can_navigate_to("unknown-screen").unwrap());
    }

    #[test]
    fn test_get_all_features() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        let features = service.get_all_features().unwrap();

        assert_eq!(features.len(), 3);
        assert!(features.iter().any(|f| f.feature_id == "checkout"));
        assert!(features.iter().any(|f| f.feature_id == "returns"));
        assert!(features.iter().any(|f| f.feature_id == "drafts"));
    }

    #[test]
    fn test_get_enabled_features() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        let features = service.get_enabled_features().unwrap();

        assert_eq!(features.len(), 2);
        assert!(features.iter().any(|f| f.feature_id == "checkout"));
        assert!(features.iter().any(|f| f.feature_id == "returns"));
        assert!(!features.iter().any(|f| f.feature_id == "drafts"));
    }

    #[test]
    fn test_get_feature_screens() {
        let (service, db) = setup_test_service();
        seed_features(&db);

        let screens = service.get_feature_screens("returns").unwrap();

        assert_eq!(screens.len(), 3);
        assert!(screens.iter().any(|s| s.screen_id == "return-entry"));
        assert!(screens.iter().any(|s| s.screen_id == "return-items"));
        assert!(screens.iter().any(|s| s.screen_id == "refund"));
    }
}
