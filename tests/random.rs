mod common;

use common::{main_script, run_source};
use mlua::Table;

#[tokio::test]
async fn seeded_generators_repeat_their_sequences() {
    let dir = main_script(
        r#"
local Random = import("Random")
local first, second = Random.new(42), Random.new(42)
same = true
for _ = 1, 200 do
    if first:NextNumber() ~= second:NextNumber() or first:NextInt(-5, 5) ~= second:NextInt(-5, 5) then
        same = false
    end
end
local other = Random.new(43)
different = Random.new(42):NextNumber() ~= other:NextNumber()
local named = Random.new("world-1")
namedSeed = named.Seed
namedAgain = Random.new(namedSeed):NextInt(1, 1000000) == Random.new("world-1"):NextInt(1, 1000000)
fresh = Random.new()
freshAgain = Random.new(fresh.Seed):NextNumber() == Random.new(fresh.Seed):NextNumber()
local clone = first:Clone()
cloned = clone:NextNumber() == first:NextNumber()
local reset = Random.new(7)
local start = reset:NextNumber()
reset:NextNumber()
reset:Reset()
resetMatches = reset:NextNumber() == start
seedText = tostring(Random.new(7))
"#,
    );
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    for name in ["same", "different", "namedAgain", "freshAgain", "cloned", "resetMatches"] {
        assert!(outcome.global::<bool>(name), "{name}");
    }
    let seed: f64 = outcome.global("namedSeed");
    assert!(seed.fract() == 0.0 && (0.0..9_007_199_254_740_992.0).contains(&seed));
    assert_eq!(outcome.global::<String>("seedText"), "Random(7)");
}

#[tokio::test]
async fn numbers_stay_in_their_ranges() {
    let dir = main_script(
        r#"
local Random = import("Random")
local rng = Random.new(1)
seen = {}
ok = true
for _ = 1, 3000 do
    local value = rng:NextInt(1, 6)
    seen[value] = true
    if value < 1 or value > 6 or value % 1 ~= 0 then ok = false end
    local number = rng:NextNumber(-2, 3)
    if number < -2 or number >= 3 then ok = false end
    local unit = rng:NextNumber()
    if unit < 0 or unit >= 1 then ok = false end
    local angle = rng:NextAngle()
    if angle < 0 or angle >= 360 then ok = false end
    local sign = rng:NextSign()
    if sign ~= 1 and sign ~= -1 then ok = false end
    local direction = rng:NextDirection()
    if math.abs(direction.X * direction.X + direction.Y * direction.Y - 1) > 1e-9 or direction.Z ~= 0 then ok = false end
    local unitVector = rng:NextUnitVector()
    if math.abs(unitVector.X ^ 2 + unitVector.Y ^ 2 + unitVector.Z ^ 2 - 1) > 1e-9 then ok = false end
    local inside = rng:NextPointInCircle(udim.new(10, 20), 5)
    if (inside.X - 10) ^ 2 + (inside.Y - 20) ^ 2 > 25 + 1e-9 then ok = false end
    local edge = rng:NextPointOnCircle(udim.new(0, 0), 3)
    if math.abs(edge.X ^ 2 + edge.Y ^ 2 - 9) > 1e-9 then ok = false end
    local box = rng:NextUDim(udim.new(0, 10, -1), udim.new(5, 20, 1))
    if box.X < 0 or box.X > 5 or box.Y < 10 or box.Y > 20 or box.Z < -1 or box.Z > 1 then ok = false end
    local tint = rng:NextColor()
    if tint.R < 0 or tint.R > 1 or tint.A ~= 1 then ok = false end
end
allFaces = seen[1] and seen[2] and seen[3] and seen[4] and seen[5] and seen[6]
local sum, squares = 0, 0
for _ = 1, 20000 do
    local value = rng:NextGaussian(10, 2)
    sum += value
    squares += value * value
end
mean = sum / 20000
deviation = math.sqrt(squares / 20000 - mean * mean)
local trues = 0
for _ = 1, 10000 do
    if rng:NextBool(0.25) then trues += 1 end
end
chance = trues / 10000
never = rng:NextBool(0)
always = rng:NextBool(1)
local big = rng:NextInt(-9007199254740991, 9007199254740991)
bigOk = big % 1 == 0
exponentialOk = rng:NextExponential(2) >= 0
"#,
    );
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("ok"));
    assert!(outcome.global::<bool>("allFaces"));
    assert!((outcome.global::<f64>("mean") - 10.0).abs() < 0.1);
    assert!((outcome.global::<f64>("deviation") - 2.0).abs() < 0.1);
    assert!((outcome.global::<f64>("chance") - 0.25).abs() < 0.03);
    assert!(!outcome.global::<bool>("never"));
    assert!(outcome.global::<bool>("always"));
    assert!(outcome.global::<bool>("bigOk"));
    assert!(outcome.global::<bool>("exponentialOk"));
}

#[tokio::test]
async fn lists_are_picked_shuffled_and_sampled() {
    let dir = main_script(
        r#"
local Random = import("Random")
local rng = Random.new(99)
local list = { "a", "b", "c", "d", "e" }
local value, index = rng:Pick(list)
picked = list[index] == value
emptyPick = rng:Pick({}) == nil
local counts = { 0, 0, 0 }
for _ = 1, 6000 do
    local _, weighted = rng:WeightedPick({ "x", "y", "z" }, { 1, 0, 3 })
    counts[weighted] += 1
end
weights = counts
local shuffled = rng:Shuffle({ 1, 2, 3, 4, 5, 6, 7, 8 })
table.sort(shuffled)
shuffleKeeps = table.concat(shuffled, ",")
local sample = rng:Sample({ 1, 2, 3, 4, 5, 6, 7, 8 }, 5)
local unique = {}
sampleUnique = #sample == 5
for _, item in sample do
    if unique[item] then sampleUnique = false end
    unique[item] = true
end
uuid = rng:NextUUID()
code = rng:NextString(12)
digits = rng:NextString(8, "01")
emoji = rng:NextString(3, "🎲")
bytes = #rng:NextBytes(33)
"#,
    );
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("picked"));
    assert!(outcome.global::<bool>("emptyPick"));
    let weights: Vec<f64> = outcome.global("weights");
    assert_eq!(weights[1], 0.0);
    assert!((weights[2] / weights[0] - 3.0).abs() < 0.4, "{weights:?}");
    assert_eq!(outcome.global::<String>("shuffleKeeps"), "1,2,3,4,5,6,7,8");
    assert!(outcome.global::<bool>("sampleUnique"));
    let uuid: String = outcome.global("uuid");
    assert_eq!(uuid.len(), 36);
    assert_eq!(&uuid[14..15], "4");
    assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"));
    let code: String = outcome.global("code");
    assert!(code.len() == 12 && code.chars().all(|character| character.is_ascii_alphanumeric()));
    let digits: String = outcome.global("digits");
    assert!(digits.len() == 8 && digits.chars().all(|character| character == '0' || character == '1'));
    assert_eq!(outcome.global::<String>("emoji"), "🎲🎲🎲");
    assert_eq!(outcome.global::<i64>("bytes"), 33);
}

#[tokio::test]
async fn noise_is_smooth_seeded_and_bounded() {
    let dir = main_script(
        r#"
local Random = import("Random")
local first, second = Random.new(5), Random.new(5)
bounded = true
smooth = true
local previous = first:Noise(udim.new(0.05, 0.3, 0.7))
for step = 1, 2000 do
    local position = udim.new(step * 0.01, 0.3, 0.7)
    local value = first:Noise(position)
    if value < -1 or value > 1 then bounded = false end
    if math.abs(value - previous) > 0.1 then smooth = false end
    previous = value
    local fractal = first:FractalNoise(position, 5)
    if fractal < -1 or fractal > 1 then bounded = false end
end
same = first:Noise(udim.new(1.37, 2.71, 0)) == second:Noise(udim.new(1.37, 2.71, 0))
differs = first:Noise(udim.new(1.37, 2.71, 0)) ~= Random.new(6):Noise(udim.new(1.37, 2.71, 0))
first:NextNumber()
stable = first:Noise(udim.new(1.37, 2.71, 0)) == second:Noise(udim.new(1.37, 2.71, 0))
"#,
    );
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    for name in ["bounded", "smooth", "same", "differs", "stable"] {
        assert!(outcome.global::<bool>(name), "{name}");
    }
}

#[tokio::test]
async fn bad_arguments_explain_themselves() {
    let dir = main_script(
        r#"
local Random = import("Random")
local rng = Random.new(1)
local results = {}
for name, attempt in {
    order = function() rng:NextInt(5, 1) end,
    fraction = function() rng:NextInt(1.5, 3) end,
    chance = function() rng:NextBool(2) end,
    weights = function() rng:WeightedPick({ 1, 2 }, { 1 }) end,
    zero = function() rng:WeightedPick({ 1 }, { 0 }) end,
    sample = function() rng:Sample({ 1, 2 }, 3) end,
    seed = function() Random.new(true) end,
} do
    local ok, message = pcall(attempt)
    results[name] = if ok then "no error" else tostring(message)
end
errors = results
"#,
    );
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let errors: Table = outcome.global("errors");
    let message = |name: &str| errors.get::<String>(name).unwrap();
    assert!(message("order").contains("must not be greater than its max"));
    assert!(message("fraction").contains("whole number"));
    assert!(message("chance").contains("between 0 and 1"));
    assert!(message("weights").contains("one weight per value"));
    assert!(message("zero").contains("at least one weight above 0"));
    assert!(message("sample").contains("between 0 and 2"));
    assert!(message("seed").contains("number or a string"));
}
