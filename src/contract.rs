#[cfg(test)]
mod tests {
    use serde_json::Value;

    fn operation<'a>(operations: &'a [Value], id: &str) -> &'a Value {
        operations
            .iter()
            .find(|operation: &&Value| operation["operationId"] == id)
            .expect("operation in pinned contract")
    }

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

    #[test]
    fn organization_commands_match_pinned_d6e_auth_contract() {
        let contract: Value = serde_json::from_str(include_str!(
            "../docs/architecture/contracts/d6e-cli-user-api.v1.json"
        ))
        .expect("valid contract JSON");
        let operations: &[Value] = contract["operations"].as_array().expect("operations array");
        for (id, method, path) in [
            ("organizations.list", "GET", "/organizations"),
            ("organizations.create", "POST", "/organizations"),
            (
                "organizations.get",
                "GET",
                "/organizations/{organizationId}",
            ),
            (
                "organizations.updateName",
                "PATCH",
                "/organizations/{organizationId}",
            ),
            (
                "organizationProfile.get",
                "GET",
                "/organizations/{organizationId}/profile",
            ),
            (
                "organizationProfile.patch",
                "PATCH",
                "/organizations/{organizationId}/profile",
            ),
        ] {
            let entry: &Value = operation(operations, id);
            assert_eq!(entry["method"], method);
            assert_eq!(entry["path"], path);
        }
        assert_eq!(
            operation(operations, "organizations.updateName")["request"]["allowedFields"],
            serde_json::json!(["name"])
        );
        assert_eq!(
            operation(operations, "organizationProfile.get")["response"]["requiredHeaders"],
            serde_json::json!(["ETag"])
        );
        assert_eq!(
            operation(operations, "organizationProfile.patch")["request"]["requiredHeaders"],
            serde_json::json!(["If-Match"])
        );
        assert_eq!(
            operation(operations, "organizationProfile.patch")["response"]["staleStatus"],
            409
        );
    }

    #[test]
    fn auth_client_commands_match_pinned_d6e_auth_contract() {
        let contract: Value = serde_json::from_str(include_str!(
            "../docs/architecture/contracts/d6e-cli-user-api.v1.json"
        ))
        .expect("valid contract JSON");
        let operations: &[Value] = contract["operations"].as_array().expect("operations array");
        for (id, method, path) in [
            (
                "authClients.list",
                "GET",
                "/organizations/{organizationId}/auth-clients",
            ),
            (
                "authClients.get",
                "GET",
                "/organizations/{organizationId}/auth-clients/{clientId}",
            ),
            (
                "authClients.create",
                "POST",
                "/organizations/{organizationId}/auth-clients",
            ),
            (
                "authClients.update",
                "PATCH",
                "/organizations/{organizationId}/auth-clients/{clientId}",
            ),
            (
                "authClients.revoke",
                "PATCH",
                "/organizations/{organizationId}/auth-clients/{clientId}",
            ),
            (
                "authClients.rotateSecret",
                "POST",
                "/organizations/{organizationId}/auth-clients/{clientId}/secret",
            ),
        ] {
            let entry: &Value = operation(operations, id);
            assert_eq!(entry["method"], method);
            assert_eq!(entry["path"], path);
        }
        assert_eq!(
            operation(operations, "authClients.revoke")["request"]["exactBody"],
            serde_json::json!({"status":"inactive"})
        );
        assert_eq!(
            operation(operations, "authClients.rotateSecret")["response"]["oneTimeSecretField"],
            "clientSecret"
        );
        for id in [
            "authClients.list",
            "authClients.get",
            "authClients.update",
            "authClients.revoke",
        ] {
            assert_eq!(
                operation(operations, id)["response"]["containsSecret"],
                false
            );
        }
    }
}
