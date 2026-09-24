#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn personal_commands_match_pinned_d6e_auth_contract() {
        let contract: Value = serde_json::from_str(include_str!(
            "../docs/architecture/contracts/d6e-cli-user-api.v1.json"
        ))
        .expect("valid contract JSON");
        assert_eq!(contract["version"], 1);
        assert_eq!(contract["basePath"], "/api/v1");
        assert_eq!(contract["authentication"]["audience"], "d6e-cli");
        assert_eq!(contract["authentication"]["tokenKind"], "access");
        assert_eq!(contract["authentication"]["refreshTokensAccepted"], false);

        let operations: &[Value] = contract["operations"].as_array().expect("operations array");
        let get: &Value = operations
            .iter()
            .find(|operation: &&Value| operation["operationId"] == "me.get")
            .expect("me.get operation");
        let update: &Value = operations
            .iter()
            .find(|operation: &&Value| operation["operationId"] == "me.updateName")
            .expect("me.updateName operation");
        assert_eq!(get["method"], "GET");
        assert_eq!(get["path"], "/me");
        assert_eq!(update["method"], "PATCH");
        assert_eq!(update["path"], "/me");
        assert_eq!(
            update["request"]["allowedFields"],
            serde_json::json!(["name"])
        );
        assert_eq!(update["request"]["name"]["maxLength"], 100);
    }
}
