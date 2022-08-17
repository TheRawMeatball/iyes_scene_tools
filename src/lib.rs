use bevy::ecs::all_tuples;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;

use bevy::ecs::component::ComponentId;
use bevy::ecs::query::ReadOnlyWorldQuery;
use bevy::reflect::TypeRegistry;
use bevy::scene::DynamicEntity;
use bevy::utils::{HashMap, HashSet};

/// Create a Bevy Dynamic Scene with specific entities and components.
///
/// The two generic parameters are treated the same way as with Bevy `Query`.
///
/// The created scene will include only the entities that match the query,
/// and only the set of components that are included in the query and impl `Reflect`.
///
/// If you want to include all components, try:
///  - [`scene_from_query`]
///
/// If what you need cannot be expressed with just a query,
/// try [`SceneBuilder`].
pub fn scene_from_query_components<Q, F>(
    world: &mut World,
) -> DynamicScene
where
    Q: ComponentList,
    F: ReadOnlyWorldQuery + 'static,
{
    let mut ss = SystemState::<Query<Entity, (Q::QueryFilter, F)>>::new(world);
    let type_registry = world.get_resource::<TypeRegistry>().unwrap().read();
    let q = ss.get(world);

    let entities = q.iter().map(|entity| {
        let get_reflect_by_id = |id|
            world.components()
                .get_info(id)
                .and_then(|info| type_registry.get(info.type_id().unwrap()))
                .and_then(|reg| reg.data::<ReflectComponent>())
                .and_then(|rc| rc.reflect(world, entity))
                .map(|c| c.clone_value());

        // TODO: avoid this allocation somehow?
        let mut ids = Vec::new();
        Q::do_component_ids(world, &mut |id| ids.push(id));

        let components = ids.into_iter()
            .filter_map(get_reflect_by_id)
            .collect();

        DynamicEntity {
            entity: entity.id(),
            components,
        }
    }).collect();

    DynamicScene {
        entities,
    }
}

/// Create a Bevy Dynamic Scene with specific entities.
///
/// The generic parameter is used as a `Query` filter.
///
/// The created scene will include only the entities that match the query
/// filter provided. All components that impl `Reflect` will be included.
///
/// If you only want specific components, try:
///  - [`scene_from_query_components`]
///
/// If what you need cannot be expressed with just a query filter,
/// try [`SceneBuilder`].
pub fn scene_from_query_filter<F>(
    world: &mut World,
) -> DynamicScene
where
    F: ReadOnlyWorldQuery + 'static,
{
    let mut ss = SystemState::<Query<Entity, F>>::new(world);
    let type_registry = world.get_resource::<TypeRegistry>().unwrap().read();
    let q = ss.get(world);

    let entities = q.iter().map(|entity| {
        let get_reflect_by_id = |id|
            world.components()
                .get_info(id)
                .and_then(|info| type_registry.get(info.type_id().unwrap()))
                .and_then(|reg| reg.data::<ReflectComponent>())
                .and_then(|rc| rc.reflect(world, entity))
                .map(|c| c.clone_value());

        let components = world.entities()
            .get(entity)
            .and_then(|eloc| world.archetypes().get(eloc.archetype_id))
            .into_iter()
            .flat_map(|a| a.components())
            .filter_map(get_reflect_by_id)
            .collect();

        DynamicEntity {
            entity: entity.id(),
            components,
        }
    }).collect();

    DynamicScene {
        entities,
    }
}

enum ComponentSelection {
    All,
    ByIds(HashSet<ComponentId>),
}

/// Flexible tool for creating Bevy scenes
///
/// You can select what entities from your `World` you would like
/// to include in the scene, by adding them using the various methods.
///
/// For each entity, you can choose whether you would like to include
/// all components (that impl `Reflect`) or just a specific set.
///
/// See the documentation of the various methods for more info.
///
/// After you are done adding entities and components, you can call
/// `.build_scene(...)` to create a [`DynamicScene`] with everything
/// that was selected.
pub struct SceneBuilder<'w> {
    world: &'w mut World,
    ec: HashMap<Entity, ComponentSelection>,
}

impl<'w> SceneBuilder<'w> {
    /// Create a new scene builder
    ///
    /// The entities and components of the created scene will come from
    /// the provided `world`.
    pub fn new(world: &'w mut World) -> SceneBuilder<'w> {
        SceneBuilder {
            world,
            ec: Default::default(),
        }
    }

    /// Add all entities that match the given query filter
    ///
    /// This method allows you to select entities in a way similar to
    /// using Bevy query filters.
    ///
    /// All components of each entity will be included.
    ///
    /// If you want to only include specific components, try:
    ///  - [`add_with_components`]
    pub fn add_from_query_filter<F>(&mut self) -> &mut Self
    where
        F: ReadOnlyWorldQuery + 'static,
    {
        let mut ss = SystemState::<Query<Entity, F>>::new(self.world);
        let q = ss.get(self.world);
        for e in q.iter() {
            self.ec.insert(e, ComponentSelection::All);
        }
        self
    }

    /// Add a specific entity
    ///
    /// The entity ID provided will be added, if it has not been already.
    ///
    /// All components of the entity will be included.
    ///
    /// If you want to only include specific components, try:
    ///  - [`add_components_to_entity`]
    pub fn add_entity(&mut self, e: Entity) -> &mut Self {
        self.ec.insert(e, ComponentSelection::All);
        self
    }

    /// Include the specified components on a given entity ID
    ///
    /// The entity ID provided will be added, if it has not been already.
    ///
    /// The components listed in `Q` will be added its component selection.
    ///
    /// If you want to select all components, try:
    ///  - [`add_entity`]
    pub fn add_components_to_entity<Q>(&mut self, e: Entity) -> &mut Self
    where
        Q: ComponentList,
    {
        if let Some(item) = self.ec.get_mut(&e) {
            if let ComponentSelection::ByIds(c) = item {
                Q::do_component_ids(self.world, &mut |id| {c.insert(id);});
            }
        } else {
            let mut c = HashSet::default();
            Q::do_component_ids(self.world, &mut |id| {c.insert(id);});
            self.ec.insert(e, ComponentSelection::ByIds(c));
        }
        self
    }

    /// Add entities by ID
    ///
    /// The entity IDs provided will be added, if they have not been already.
    ///
    /// All components of each entity will be included.
    ///
    /// If you want to only include specific components, try:
    ///  - [`add_components_to_entities`]
    pub fn add_entities<I>(&mut self, entities: I) -> &mut Self
    where
        I: IntoIterator<Item = Entity>,
    {
        for e in entities {
            self.ec.insert(e, ComponentSelection::All);
        }
        self
    }

    /// Include the specified components to entities with ID
    ///
    /// The entity IDs provided will be added, if they have not been already.
    ///
    /// The components listed in `Q` will be added their component selections.
    ///
    /// If you want to select all components, try:
    ///  - [`add_entities`]
    pub fn add_components_to_entities<I, Q>(&mut self, entities: I) -> &mut Self
    where
        I: IntoIterator<Item = Entity>,
        Q: ComponentList,
    {
        for e in entities {
            if let Some(item) = self.ec.get_mut(&e) {
                if let ComponentSelection::ByIds(c) = item {
                    Q::do_component_ids(self.world, &mut |id| {c.insert(id);});
                }
            } else {
                let mut c = HashSet::default();
                Q::do_component_ids(self.world, &mut |id| {c.insert(id);});
                self.ec.insert(e, ComponentSelection::ByIds(c));
            }
        }
        self
    }

    /// Add specific components to entities that match a query filter
    ///
    /// This method allows you to select entities in a way similar to
    /// using Bevy query filters.
    ///
    /// The components listed in `Q` will be added to each of the entities.
    ///
    /// If you want to select all components, try:
    ///  - [`add_from_query_filter`]
    pub fn add_with_components<Q, F>(&mut self) -> &mut Self
    where
        Q: ComponentList,
        F: ReadOnlyWorldQuery + 'static,
    {
        let mut ss = SystemState::<Query<Entity, (Q::QueryFilter, F)>>::new(self.world);
        let q = ss.get(self.world);
        for e in q.iter() {
            if let Some(item) = self.ec.get_mut(&e) {
                if let ComponentSelection::ByIds(c) = item {
                    Q::do_component_ids(self.world, &mut |id| {c.insert(id);});
                }
            } else {
                let mut c = HashSet::default();
                Q::do_component_ids(self.world, &mut |id| {c.insert(id);});
                self.ec.insert(e, ComponentSelection::ByIds(c));
            }
        }
        self
    }

    /// Build a [`DynamicScene`] with the selected entities and components
    ///
    /// Everything that was added to the builder (using the various `add_*`
    /// methods) will be included in the scene.
    ///
    /// All the relevant data will be copied from the `World` that was provided
    /// when the [`SceneBuilder`] was created.
    pub fn build_scene(&self) -> DynamicScene {
        let type_registry = self.world.get_resource::<TypeRegistry>().unwrap().read();

        let entities = self.ec.iter().map(|(entity, csel)| {
            let get_reflect_by_id = |id|
                self.world.components()
                    .get_info(id)
                    .and_then(|info| type_registry.get(info.type_id().unwrap()))
                    .and_then(|reg| reg.data::<ReflectComponent>())
                    .and_then(|rc| rc.reflect(self.world, *entity))
                    .map(|c| c.clone_value());

            let components = match csel {
                ComponentSelection::All => {
                    self.world.entities()
                        .get(*entity)
                        .and_then(|eloc| self.world.archetypes().get(eloc.archetype_id))
                        .into_iter()
                        .flat_map(|a| a.components())
                        .filter_map(get_reflect_by_id)
                        .collect()
                },
                ComponentSelection::ByIds(ids) => {
                    ids.iter()
                        .cloned()
                        .filter_map(get_reflect_by_id)
                        .collect()
                },
            };

            DynamicEntity {
                entity: entity.id(),
                components,
            }
        }).collect();

        DynamicScene {
            entities,
        }
    }
}

pub trait ComponentList {
    type QueryFilter: ReadOnlyWorldQuery + 'static;
    fn do_component_ids<F: FnMut(ComponentId)>(world: &World, f: &mut F);
}

impl<T: Component + Reflect> ComponentList for &T {
    type QueryFilter = With<T>;
    #[inline]
    fn do_component_ids<F: FnMut(ComponentId)>(world: &World, f: &mut F) {
        f(world.component_id::<T>().expect("Component not in World"));
    }
}

macro_rules! componentlist_impl {
    ($($x:ident),*) => {
        impl<$($x: ComponentList),*> ComponentList for ($($x,)*) {
            type QueryFilter = ($($x::QueryFilter,)*);
            #[inline]
            fn do_component_ids<F: FnMut(ComponentId)>(_world: &World, _f: &mut F) {
                $($x::do_component_ids(_world, _f);)*
            }
        }
    };
}

all_tuples!(componentlist_impl, 0, 15, T);

#[cfg(test)]
mod test {
}
