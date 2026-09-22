use std::collections::HashSet;
use std::rc::Rc;

use mlua::{AnyUserData, Lua, MultiValue, Result, Table, Value};

use crate::datatypes::UDim;
use crate::graphics::geometry::{Hit, Query, valid_query};
use crate::graphics::protocol::ObjectId;
use crate::objects::{Renderable, Scene};

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[derive(Default)]
struct Filter {
    include: Option<HashSet<ObjectId>>,
    exclude: HashSet<ObjectId>,
}

impl Filter {
    fn allows(&self, id: ObjectId) -> bool {
        !self.exclude.contains(&id) && self.include.as_ref().is_none_or(|include| include.contains(&id))
    }
}

fn identities(value: Value, key: &str) -> Result<HashSet<ObjectId>> {
    let Value::Table(list) = value else {
        return Err(runtime(format!("{key} must be an array of renderables, got {}", value.type_name())));
    };
    list.sequence_values::<Value>()
        .map(|item| match item? {
            Value::UserData(userdata) => userdata
                .borrow::<Renderable>()
                .map(|renderable| renderable.id())
                .map_err(|_| runtime(format!("{key} must only hold renderables"))),
            other => Err(runtime(format!("{key} must only hold renderables, got {}", other.type_name()))),
        })
        .collect()
}

fn filter(params: Option<Table>) -> Result<Filter> {
    let mut filter = Filter::default();
    let Some(params) = params else {
        return Ok(filter);
    };
    let include: Value = params.get("Include")?;
    if !include.is_nil() {
        filter.include = Some(identities(include, "Include")?);
    }
    let exclude: Value = params.get("Exclude")?;
    if !exclude.is_nil() {
        filter.exclude = identities(exclude, "Exclude")?;
    }
    Ok(filter)
}

async fn search(scene: &Scene, query: Query, params: Option<Table>) -> Result<Vec<(AnyUserData, Hit)>> {
    if !valid_query(&query) {
        return Err(runtime(
            "queries need finite positions and sizes, a radius of at least 0 and a direction that is not zero",
        ));
    }
    let filter = filter(params)?;
    let hits = scene.query(query).await.map_err(runtime)?;
    let mut found: Vec<(AnyUserData, Hit, f64, u64)> = hits
        .into_iter()
        .filter(|hit| filter.allows(hit.id))
        .filter_map(|hit| {
            let userdata = scene.userdata(hit.id)?;
            let ordering = scene.ordering(hit.id)?;
            Some((userdata, hit, ordering.z_index, ordering.order))
        })
        .collect();
    let ray = matches!(query, Query::Ray { .. });
    found.sort_by(|a, b| {
        let topmost = b.2.total_cmp(&a.2).then(b.3.cmp(&a.3));
        if ray {
            a.1.distance.total_cmp(&b.1.distance).then(topmost)
        } else {
            topmost
        }
    });
    Ok(found.into_iter().map(|(userdata, hit, _, _)| (userdata, hit)).collect())
}

fn objects(lua: &Lua, found: Vec<(AnyUserData, Hit)>) -> Result<Table> {
    lua.create_sequence_from(found.into_iter().map(|(userdata, _)| userdata))
}

fn raycast_result(lua: &Lua, userdata: AnyUserData, hit: Hit) -> Result<Table> {
    let result = lua.create_table()?;
    result.set("Renderable", userdata)?;
    result.set("Position", UDim::new(hit.position[0] + 0.0, hit.position[1] + 0.0, 0.0))?;
    result.set("Normal", UDim::new(hit.normal[0] + 0.0, hit.normal[1] + 0.0, 0.0))?;
    result.set("Distance", hit.distance)?;
    Ok(result)
}

fn area_arguments(rest: MultiValue) -> Result<(f64, Option<Table>)> {
    let mut rest = rest.into_iter();
    let params = |rest: &mut dyn Iterator<Item = Value>| rest.next().and_then(|value| value.as_table().cloned());
    match rest.next() {
        None | Some(Value::Nil) => Ok((0.0, params(&mut rest))),
        Some(Value::Integer(rotation)) => Ok((rotation as f64, params(&mut rest))),
        Some(Value::Number(rotation)) => Ok((rotation, params(&mut rest))),
        Some(Value::Table(table)) => Ok((0.0, Some(table))),
        Some(other) => Err(runtime(format!(
            "QueryArea takes an optional rotation and query params, got {}",
            other.type_name()
        ))),
    }
}

fn ray(origin: UDim, direction: UDim) -> Query {
    Query::Ray {
        origin: [origin.x, origin.y],
        direction: [direction.x, direction.y],
    }
}

pub fn create(lua: &Lua, scene: &Rc<Scene>) -> Result<Table> {
    let api = lua.create_table()?;

    let owner = scene.clone();
    api.set(
        "new",
        lua.create_function(move |lua, (class, config): (String, Option<Table>)| {
            Renderable::create(lua, &owner, &class, config)
        })?,
    )?;

    let owner = scene.clone();
    api.set(
        "QueryPoint",
        lua.create_async_function(move |lua, (point, params): (UDim, Option<Table>)| {
            let scene = owner.clone();
            async move { objects(&lua, search(&scene, Query::Point([point.x, point.y]), params).await?) }
        })?,
    )?;

    let owner = scene.clone();
    api.set(
        "QueryArea",
        lua.create_async_function(move |lua, (center, size, rest): (UDim, UDim, MultiValue)| {
            let scene = owner.clone();
            async move {
                let (rotation, params) = area_arguments(rest)?;
                let query = Query::Area {
                    center: [center.x, center.y],
                    size: [size.x, size.y],
                    rotation: rotation.to_radians(),
                };
                objects(&lua, search(&scene, query, params).await?)
            }
        })?,
    )?;

    let owner = scene.clone();
    api.set(
        "QueryRadius",
        lua.create_async_function(move |lua, (center, radius, params): (UDim, f64, Option<Table>)| {
            let scene = owner.clone();
            async move {
                let query = Query::Radius {
                    center: [center.x, center.y],
                    radius,
                };
                objects(&lua, search(&scene, query, params).await?)
            }
        })?,
    )?;

    let owner = scene.clone();
    api.set(
        "Raycast",
        lua.create_async_function(move |lua, (origin, direction, params): (UDim, UDim, Option<Table>)| {
            let scene = owner.clone();
            async move {
                match search(&scene, ray(origin, direction), params).await?.into_iter().next() {
                    Some((userdata, hit)) => Ok(Value::Table(raycast_result(&lua, userdata, hit)?)),
                    None => Ok(Value::Nil),
                }
            }
        })?,
    )?;

    let owner = scene.clone();
    api.set(
        "RaycastAll",
        lua.create_async_function(move |lua, (origin, direction, params): (UDim, UDim, Option<Table>)| {
            let scene = owner.clone();
            async move {
                let results = lua.create_table()?;
                for (userdata, hit) in search(&scene, ray(origin, direction), params).await? {
                    results.push(raycast_result(&lua, userdata, hit)?)?;
                }
                Ok(results)
            }
        })?,
    )?;

    let owner = scene.clone();
    api.set(
        "GetRenderables",
        lua.create_function(move |lua, ()| lua.create_sequence_from(owner.renderables()))?,
    )?;

    api.set_readonly(true);
    Ok(api)
}
