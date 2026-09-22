use std::cell::OnceCell;
use std::f64::consts::TAU;
use std::rc::Rc;

use mlua::{Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataFields, UserDataMethods, Value};

use crate::datatypes::{Color, UDim};

const ALPHANUMERIC: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const SEED_MASK: u64 = (1 << 53) - 1;
const LARGEST_WHOLE: f64 = 9_007_199_254_740_992.0;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const NOISE_SALT: u64 = 0x6a09_e667_f3bc_c909;

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = *state;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[derive(Clone)]
struct Xoshiro([u64; 4]);

impl Xoshiro {
    fn seeded(seed: u64) -> Xoshiro {
        let mut state = seed;
        Xoshiro([splitmix(&mut state), splitmix(&mut state), splitmix(&mut state), splitmix(&mut state)])
    }

    fn next(&mut self) -> u64 {
        let state = &mut self.0;
        let result = state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shifted = state[1] << 17;
        state[2] ^= state[0];
        state[3] ^= state[1];
        state[1] ^= state[2];
        state[0] ^= state[3];
        state[2] ^= shifted;
        state[3] = state[3].rotate_left(45);
        result
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / LARGEST_WHOLE
    }

    fn below(&mut self, range: u64) -> u64 {
        let mut product = u128::from(self.next()) * u128::from(range);
        let mut low = product as u64;
        if low < range {
            let threshold = range.wrapping_neg() % range;
            while low < threshold {
                product = u128::from(self.next()) * u128::from(range);
                low = product as u64;
            }
        }
        (product >> 64) as u64
    }

    fn fill(&mut self, bytes: &mut [u8]) {
        for chunk in bytes.chunks_mut(8) {
            let word = self.next().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
    }
}

struct Noise {
    table: [u8; 512],
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

fn gradient(hash: u8, x: f64, y: f64, z: f64) -> f64 {
    let hash = hash & 15;
    let u = if hash < 8 { x } else { y };
    let v = if hash < 4 {
        y
    } else if hash == 12 || hash == 14 {
        x
    } else {
        z
    };
    (if hash & 1 == 0 { u } else { -u }) + (if hash & 2 == 0 { v } else { -v })
}

impl Noise {
    fn new(seed: u64) -> Noise {
        let mut permutation: [u8; 256] = std::array::from_fn(|index| index as u8);
        let mut state = seed ^ NOISE_SALT;
        for index in (1..256).rev() {
            let other = (splitmix(&mut state) % (index as u64 + 1)) as usize;
            permutation.swap(index, other);
        }
        Noise {
            table: std::array::from_fn(|index| permutation[index & 255]),
        }
    }

    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let (fx, fy, fz) = (x.floor(), y.floor(), z.floor());
        let (x, y, z) = (x - fx, y - fy, z - fz);
        let cell = |value: f64| (value.rem_euclid(256.0) as usize) & 255;
        let (xi, yi, zi) = (cell(fx), cell(fy), cell(fz));
        let (u, v, w) = (fade(x), fade(y), fade(z));
        let table = &self.table;
        let a = table[xi] as usize + yi;
        let aa = table[a] as usize + zi;
        let ab = table[a + 1] as usize + zi;
        let b = table[xi + 1] as usize + yi;
        let ba = table[b] as usize + zi;
        let bb = table[b + 1] as usize + zi;
        let value = lerp(
            w,
            lerp(
                v,
                lerp(u, gradient(table[aa], x, y, z), gradient(table[ba], x - 1.0, y, z)),
                lerp(u, gradient(table[ab], x, y - 1.0, z), gradient(table[bb], x - 1.0, y - 1.0, z)),
            ),
            lerp(
                v,
                lerp(
                    u,
                    gradient(table[aa + 1], x, y, z - 1.0),
                    gradient(table[ba + 1], x - 1.0, y, z - 1.0),
                ),
                lerp(
                    u,
                    gradient(table[ab + 1], x, y - 1.0, z - 1.0),
                    gradient(table[bb + 1], x - 1.0, y - 1.0, z - 1.0),
                ),
            ),
        );
        value.clamp(-1.0, 1.0)
    }
}

#[derive(Clone)]
pub struct Random {
    seed: f64,
    start: u64,
    state: Xoshiro,
    noise: OnceCell<Rc<Noise>>,
}

fn entropy() -> u64 {
    let mut bytes = [0u8; 8];
    if aws_lc_rs::rand::fill(&mut bytes).is_err() {
        let mut time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or_default();
        return splitmix(&mut time);
    }
    u64::from_le_bytes(bytes)
}

fn whole(what: &str, value: f64) -> Result<i64> {
    if value.is_finite() && value.fract() == 0.0 && value.abs() <= LARGEST_WHOLE {
        Ok(value as i64)
    } else {
        Err(runtime(format!("{what} must be a whole number, got {value}")))
    }
}

fn amount(what: &str, value: f64, limit: usize) -> Result<usize> {
    let number = whole(what, value)?;
    if number < 0 || number as u64 > limit as u64 {
        return Err(runtime(format!("{what} must be between 0 and {limit}, got {value}")));
    }
    Ok(number as usize)
}

impl Random {
    pub const TYPE_NAME: &'static str = "Random";

    fn new(seed: Value) -> Result<Random> {
        let (seed, start) = match seed {
            Value::Nil => {
                let start = entropy() & SEED_MASK;
                (start as f64, start)
            }
            Value::Integer(number) => (number as f64, number as u64),
            Value::Number(number) if !number.is_finite() => {
                return Err(runtime("a Random seed must be a finite number or a string"));
            }
            Value::Number(number) if number.fract() == 0.0 && number.abs() <= LARGEST_WHOLE => {
                (number, number as i64 as u64)
            }
            Value::Number(number) => (number, number.to_bits()),
            Value::String(text) => {
                let start = fnv(&text.as_bytes()) & SEED_MASK;
                (start as f64, start)
            }
            other => {
                return Err(runtime(format!(
                    "a Random seed must be a number or a string, got {}",
                    other.type_name()
                )));
            }
        };
        Ok(Random {
            seed,
            start,
            state: Xoshiro::seeded(start),
            noise: OnceCell::new(),
        })
    }

    fn number(&mut self, min: f64, max: f64) -> f64 {
        min + self.state.unit() * (max - min)
    }

    fn int(&mut self, min: f64, max: f64) -> Result<i64> {
        let (low, high) = (whole("min", min)?, whole("max", max)?);
        if low > high {
            return Err(runtime(format!("NextInt's min ({low}) must not be greater than its max ({high})")));
        }
        let span = (high - low) as u64 + 1;
        Ok(low + self.state.below(span) as i64)
    }

    fn index(&mut self, length: usize) -> usize {
        self.state.below(length as u64) as usize + 1
    }

    fn noise(&self) -> Rc<Noise> {
        self.noise.get_or_init(|| Rc::new(Noise::new(self.start))).clone()
    }

    fn fractal(&self, position: UDim, octaves: usize, persistence: f64, lacunarity: f64) -> f64 {
        let noise = self.noise();
        let (mut total, mut amplitude, mut frequency, mut weight) = (0.0, 1.0, 1.0, 0.0);
        for octave in 0..octaves {
            let offset = octave as f64 * 19.19;
            total += amplitude
                * noise.sample(
                    position.x * frequency + offset,
                    position.y * frequency + offset,
                    position.z * frequency + offset,
                );
            weight += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }
        if weight == 0.0 { 0.0 } else { (total / weight).clamp(-1.0, 1.0) }
    }

    fn uuid(&mut self) -> String {
        let mut bytes = [0u8; 16];
        self.state.fill(&mut bytes);
        format_uuid(bytes)
    }
}

pub fn format_uuid(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn list_length(list: &Table) -> usize {
    list.raw_len()
}

impl UserData for Random {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, Self::TYPE_NAME);
        fields.add_field_method_get("Seed", |_, this| Ok(this.seed));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("NextInt", |_, this, (min, max): (f64, f64)| this.int(min, max));
        methods.add_method_mut("NextNumber", |_, this, (min, max): (Option<f64>, Option<f64>)| {
            let (min, max) = match (min, max) {
                (None, None) => (0.0, 1.0),
                (Some(max), None) => (0.0, max),
                (Some(min), Some(max)) => (min, max),
                (None, Some(max)) => (0.0, max),
            };
            if !min.is_finite() || !max.is_finite() {
                return Err(runtime("NextNumber needs finite numbers"));
            }
            Ok(this.number(min, max))
        });
        methods.add_method_mut("NextBool", |_, this, chance: Option<f64>| {
            let chance = chance.unwrap_or(0.5);
            if !(0.0..=1.0).contains(&chance) {
                return Err(runtime(format!("NextBool's chance must be between 0 and 1, got {chance}")));
            }
            Ok(this.state.unit() < chance)
        });
        methods.add_method_mut("NextSign", |_, this, ()| Ok(if this.state.next() & 1 == 0 { -1 } else { 1 }));
        methods.add_method_mut("NextGaussian", |_, this, (mean, deviation): (Option<f64>, Option<f64>)| {
            let first = 1.0 - this.state.unit();
            let second = this.state.unit();
            let standard = (-2.0 * first.ln()).sqrt() * (TAU * second).cos();
            Ok(mean.unwrap_or(0.0) + standard * deviation.unwrap_or(1.0))
        });
        methods.add_method_mut("NextExponential", |_, this, rate: Option<f64>| {
            let rate = rate.unwrap_or(1.0);
            if !(rate.is_finite() && rate > 0.0) {
                return Err(runtime(format!("NextExponential's rate must be greater than 0, got {rate}")));
            }
            Ok(-(1.0 - this.state.unit()).ln() / rate)
        });
        methods.add_method_mut("NextAngle", |_, this, ()| Ok(this.state.unit() * 360.0));
        methods.add_method_mut("NextDirection", |_, this, ()| {
            let angle = this.state.unit() * TAU;
            Ok(UDim::new(angle.cos(), angle.sin(), 0.0))
        });
        methods.add_method_mut("NextUnitVector", |_, this, ()| {
            let z = this.state.unit() * 2.0 - 1.0;
            let angle = this.state.unit() * TAU;
            let radius = (1.0 - z * z).max(0.0).sqrt();
            Ok(UDim::new(radius * angle.cos(), radius * angle.sin(), z))
        });
        methods.add_method_mut("NextUDim", |_, this, (min, max): (UDim, UDim)| {
            let x = this.number(min.x, max.x);
            let y = this.number(min.y, max.y);
            let z = this.number(min.z, max.z);
            Ok(UDim::new(x, y, z))
        });
        methods.add_method_mut("NextPointInCircle", |_, this, (center, radius): (UDim, f64)| {
            let distance = radius * this.state.unit().sqrt();
            let angle = this.state.unit() * TAU;
            Ok(UDim::new(center.x + distance * angle.cos(), center.y + distance * angle.sin(), center.z))
        });
        methods.add_method_mut("NextPointOnCircle", |_, this, (center, radius): (UDim, f64)| {
            let angle = this.state.unit() * TAU;
            Ok(UDim::new(center.x + radius * angle.cos(), center.y + radius * angle.sin(), center.z))
        });
        methods.add_method_mut("NextColor", |_, this, alpha: Option<f64>| {
            let (r, g, b) = (this.state.unit(), this.state.unit(), this.state.unit());
            Ok(Color::new(r, g, b, alpha.unwrap_or(1.0)))
        });
        methods.add_method_mut("NextHue", |_, this, (saturation, value): (Option<f64>, Option<f64>)| {
            let hue = this.state.unit();
            Ok(Color::from_hsv(hue, saturation.unwrap_or(1.0), value.unwrap_or(1.0), 1.0))
        });
        methods.add_method_mut("NextBytes", |lua, this, count: f64| {
            let count = amount("the byte count", count, MAX_BYTES)?;
            let mut bytes = vec![0u8; count];
            this.state.fill(&mut bytes);
            lua.create_string(&bytes)
        });
        methods.add_method_mut("NextString", |lua, this, (length, characters): (f64, Option<String>)| {
            let length = amount("the length", length, MAX_BYTES)?;
            let pool: Vec<char> = characters.as_deref().unwrap_or(ALPHANUMERIC).chars().collect();
            if pool.is_empty() {
                return Err(runtime("NextString needs at least one character to choose from"));
            }
            let text: String = (0..length)
                .map(|_| pool[this.state.below(pool.len() as u64) as usize])
                .collect();
            lua.create_string(&text)
        });
        methods.add_method_mut("NextUUID", |_, this, ()| Ok(this.uuid()));
        methods.add_method_mut("Pick", |_, this, list: Table| {
            let length = list_length(&list);
            if length == 0 {
                return Ok((Value::Nil, Value::Nil));
            }
            let index = this.index(length);
            Ok((list.raw_get::<Value>(index)?, Value::Integer(index as i64)))
        });
        methods.add_method_mut("WeightedPick", |_, this, (list, weights): (Table, Table)| {
            let length = list_length(&list);
            if list_length(&weights) != length {
                return Err(runtime(format!(
                    "WeightedPick needs one weight per value, got {} values and {} weights",
                    length,
                    list_length(&weights)
                )));
            }
            let mut table = Vec::with_capacity(length);
            let mut total = 0.0;
            for index in 1..=length {
                let weight: f64 = weights.raw_get(index)?;
                if !(weight.is_finite() && weight >= 0.0) {
                    return Err(runtime(format!("weight #{index} must be a number of at least 0, got {weight}")));
                }
                total += weight;
                table.push(weight);
            }
            if total <= 0.0 {
                return Err(runtime("WeightedPick needs at least one weight above 0"));
            }
            let target = this.state.unit() * total;
            let mut running = 0.0;
            let mut chosen = None;
            for (index, weight) in table.iter().enumerate() {
                if *weight <= 0.0 {
                    continue;
                }
                running += weight;
                chosen = Some(index + 1);
                if target < running {
                    break;
                }
            }
            let index = chosen.unwrap_or(length);
            Ok((list.raw_get::<Value>(index)?, index))
        });
        methods.add_method_mut("Shuffle", |_, this, list: Table| {
            let length = list_length(&list);
            for index in (2..=length).rev() {
                let other = this.index(index);
                if other != index {
                    let (a, b): (Value, Value) = (list.raw_get(index)?, list.raw_get(other)?);
                    list.raw_set(index, b)?;
                    list.raw_set(other, a)?;
                }
            }
            Ok(list)
        });
        methods.add_method_mut("Sample", |lua, this, (list, count): (Table, f64)| {
            let length = list_length(&list);
            let count = amount("the sample size", count, length)?;
            let mut indices: Vec<usize> = (1..=length).collect();
            let picked = lua.create_table_with_capacity(count, 0)?;
            for slot in 0..count {
                let other = slot + this.state.below((length - slot) as u64) as usize;
                indices.swap(slot, other);
                picked.raw_set(slot + 1, list.raw_get::<Value>(indices[slot])?)?;
            }
            Ok(picked)
        });
        methods.add_method("Noise", |_, this, position: UDim| {
            Ok(this.noise().sample(position.x, position.y, position.z))
        });
        methods.add_method(
            "FractalNoise",
            |_, this, (position, octaves, persistence, lacunarity): (UDim, Option<f64>, Option<f64>, Option<f64>)| {
                let octaves = match octaves {
                    Some(octaves) => amount("the octave count", octaves, 16)?,
                    None => 4,
                };
                if octaves == 0 {
                    return Err(runtime("FractalNoise needs at least 1 octave"));
                }
                Ok(this.fractal(position, octaves, persistence.unwrap_or(0.5), lacunarity.unwrap_or(2.0)))
            },
        );
        methods.add_method("Clone", |_, this, ()| Ok(this.clone()));
        methods.add_method_mut("Reset", |_, this, ()| {
            this.state = Xoshiro::seeded(this.start);
            Ok(())
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{}({})", Self::TYPE_NAME, crate::datatypes::format_number(this.seed)))
        });
    }
}

pub fn create(lua: &Lua) -> Result<Table> {
    let random = lua.create_table()?;
    random.set(
        "new",
        lua.create_function(|_, seed: MultiValue| Random::new(seed.into_iter().next().unwrap_or(Value::Nil)))?,
    )?;
    random.set_readonly(true);
    Ok(random)
}
