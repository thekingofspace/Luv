mod common;

use common::{main_script, run_source};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn a_registry_keeps_frozen_values_under_dotted_ids() {
    let outcome = run_script(
        r#"
        local Registry = import("Registry")
        local items = Registry.new("Items")
        local changes = {}
        items.Changed:BindHandler("log", function(id, value)
            table.insert(changes, id .. "=" .. tostring(value and value.damage))
        end)

        items:Register("items.sword", { damage = 5, tags = { "sharp" } })
        items:Register("items.bow", { damage = 3, fire = function() return "twang" end })
        items:Register("itemsx", { damage = 1 })
        items:Register("items", { damage = 0 })

        local sword = items:Get("items.sword")
        local canEdit = pcall(function()
            sword.damage = 99
        end)
        local canEditNested = pcall(function()
            table.insert(sword.tags, "blunt")
        end)
        local all = items:GetAll("items")
        local listed = items:List("items")
        local removed = items:Remove("itemsx")
        local removedAgain = items:Remove("itemsx")

        items:Stage("items.axe", { damage = 7 })
        local beforeCommit = items:Has("items.axe")
        local staged = items.Staged
        local committed = items:Commit()
        items:Stage("items.spear", { damage = 9 })
        local discarded = items:Discard()

        local badId = pcall(function()
            items:Register("items..broken", 1)
        end)

        result = {
            same = Registry.new("Items") == items,
            damage = sword.damage,
            frozen = table.isfrozen(sword) and table.isfrozen(sword.tags),
            canEdit = canEdit,
            canEditNested = canEditNested,
            twang = items:Get("items.bow").fire(),
            listed = table.concat(listed, ","),
            allSword = all["items.sword"].damage,
            allHasOther = all["itemsx"] ~= nil,
            removed = removed,
            removedAgain = removedAgain,
            beforeCommit = beforeCommit,
            staged = staged,
            committed = committed,
            afterCommit = items:Get("items.axe").damage,
            discarded = discarded,
            spear = items:Has("items.spear"),
            count = items.Count,
            badId = badId,
            safe = items.IsSafe,
            changes = table.concat(changes, " "),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    let text = |key: &str| result.get::<String>(key).unwrap();
    let flag = |key: &str| result.get::<bool>(key).unwrap();
    assert!(flag("same"));
    assert_eq!(result.get::<i64>("damage").unwrap(), 5);
    assert!(flag("frozen"));
    assert!(!flag("canEdit"));
    assert!(!flag("canEditNested"));
    assert_eq!(text("twang"), "twang");
    assert_eq!(text("listed"), "items,items.bow,items.sword");
    assert_eq!(result.get::<i64>("allSword").unwrap(), 5);
    assert!(!flag("allHasOther"), "a prefix only matches whole names");
    assert!(flag("removed"));
    assert!(!flag("removedAgain"));
    assert!(!flag("beforeCommit"));
    assert_eq!(result.get::<i64>("staged").unwrap(), 1);
    assert_eq!(result.get::<i64>("committed").unwrap(), 1);
    assert_eq!(result.get::<i64>("afterCommit").unwrap(), 7);
    assert_eq!(result.get::<i64>("discarded").unwrap(), 1);
    assert!(!flag("spear"));
    assert_eq!(result.get::<i64>("count").unwrap(), 4);
    assert!(!flag("badId"));
    assert!(!flag("safe"));
    assert_eq!(
        text("changes"),
        "items.sword=5 items.bow=3 itemsx=1 items=0 itemsx=nil items.axe=7"
    );
}

#[tokio::test]
async fn a_safe_registry_is_shared_by_every_thread() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local Registry = import("Registry")
        local shared = Registry.Safe("World")
        local seen = {}
        shared.Changed:BindHandler("log", function(id, value)
            table.insert(seen, id)
        end)

        shared:Register("spawn.goblin", { health = 10, position = udim.new(1, 2) })
        local first = shared:Get("spawn.goblin")
        local cached = shared:Get("spawn.goblin") == first
        local refused, problem = pcall(function()
            shared:Register("bad", { run = function() end })
        end)

        task.parallel(function()
            local world = Registry.Safe("World")
            local goblin = world:Get("spawn.goblin")
            world:Register("spawn.orc", { health = goblin.health * 3 })
            Messenger:Fire("registered", goblin.position.Y, table.isfrozen(goblin))
        end)
        local seenY, frozenThere = Messenger:Wait("registered")
        local waited = 0
        while #seen < 2 and waited < 2 do
            waited += task.wait(0.02)
        end

        shared:Register("spawn.goblin", { health = 12 })
        result = {
            health = first.health,
            cached = cached,
            refreshed = shared:Get("spawn.goblin") ~= first and shared:Get("spawn.goblin").health == 12,
            refused = refused,
            problem = tostring(problem),
            seenY = seenY,
            frozenThere = frozenThere,
            orc = shared:Get("spawn.orc").health,
            seen = table.concat(seen, ","),
            listed = table.concat(shared:List("spawn"), ","),
            safe = shared.IsSafe,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    let flag = |key: &str| result.get::<bool>(key).unwrap();
    assert_eq!(result.get::<i64>("health").unwrap(), 10);
    assert!(flag("cached"), "reading the same version twice gives the same table");
    assert!(flag("refreshed"));
    assert!(!flag("refused"));
    assert!(result.get::<String>("problem").unwrap().contains("Registry.Safe cannot keep 'bad'"));
    assert_eq!(result.get::<f64>("seenY").unwrap(), 2.0);
    assert!(flag("frozenThere"));
    assert_eq!(result.get::<i64>("orc").unwrap(), 30);
    assert_eq!(result.get::<String>("seen").unwrap(), "spawn.goblin,spawn.orc,spawn.goblin");
    assert_eq!(result.get::<String>("listed").unwrap(), "spawn.goblin,spawn.orc");
    assert!(flag("safe"));
}
