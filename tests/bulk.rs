mod common;

use common::{main_script, run_source};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn bulk_update_sets_many_properties_in_one_call() {
    let outcome = run_script(
        r#"
        local Bulk = import("Bulk")
        local Signal = import("Signal")
        local first, second = Signal.new(), Signal.new()
        local plain = {}
        Bulk.BulkUpdate({
            [first] = { Name = "First" },
            [second] = { Name = "Second" },
            [plain] = { Visible = true, Position = udim.new(1, 2, 3), Tint = color.white },
        })
        results = {
            first = first.Name,
            second = second.Name,
            visible = plain.Visible,
            position = tostring(plain.Position),
            tint = plain.Tint:ToHex(),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("first").unwrap(), "First");
    assert_eq!(results.get::<String>("second").unwrap(), "Second");
    assert!(results.get::<bool>("visible").unwrap());
    assert_eq!(results.get::<String>("position").unwrap(), "UDim(1, 2, 3)");
    assert_eq!(results.get::<String>("tint").unwrap(), "#ffffff");
}

#[tokio::test]
async fn bulk_update_reports_bad_updates() {
    let outcome = run_script(
        r#"
        local Bulk = import("Bulk")
        local Signal = import("Signal")
        local signal = Signal.new()
        signal.Name = "Touched"
        local function failure(updates)
            local ok, message = pcall(Bulk.BulkUpdate, updates)
            assert(not ok, "expected a failure")
            return tostring(message)
        end
        errors = {
            failure({ [signal] = { Missing = 1 } }),
            failure({ [signal] = true }),
            failure({ [signal] = { [1] = "x" } }),
            failure({ [signal] = { Name = {} } }),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let errors: Vec<String> = outcome.global("errors");
    let expected = [
        "cannot set Missing on Touched",
        "the update for Touched must be a table of properties, got boolean",
        "property names must be strings, got integer for Touched",
        "cannot set Name on Touched",
    ];
    for (error, expected) in errors.iter().zip(expected) {
        assert!(error.contains(expected), "{error:?} should contain {expected:?}");
    }
}
