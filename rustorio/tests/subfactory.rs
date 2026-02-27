#![forbid(unsafe_code)]
#![forbid(internal_features)]

use rustorio::{
    HandRecipe, Resource, Technology, Tick,
    buildings::{Assembler, Furnace, Lab},
    gamemodes::Standard,
    recipes::{
        CopperSmelting, CopperWireRecipe, ElectronicCircuitRecipe, IronSmelting, RedScienceRecipe,
    },
    resources::{CopperWire, Point},
    territory::Miner,
};

type GameMode = Standard;

type StartingResources = <GameMode as rustorio::GameMode>::StartingResources;
type VictoryResources = <GameMode as rustorio::GameMode>::VictoryResources;

#[test]
fn main() {
    rustorio::play::<GameMode>(user_main);
}

use smelted_ore_factory::SmeltedOreFactory;
mod smelted_ore_factory {
    use rustorio::{
        Bundle, Resource, ResourceType, Tick,
        buildings::Furnace,
        recipes::FurnaceRecipe,
        territory::{Miner, OreType, Territory},
    };
    use rustorio_engine::subfactories::{OnEachTick, Past, PastTick, Subfactory};

    struct SmeltedOreFactoryInner<Ore: OreType, R: FurnaceRecipe> {
        // It's an `Option` so that we can move it out for handcrafting.
        territory: Option<Territory<Ore>>,
        furnace: Furnace<R>,
    }

    impl<Ore, R> OnEachTick for SmeltedOreFactoryInner<Ore, R>
    where
        Ore: OreType,
        R: FurnaceRecipe<InputBundle = (Bundle<Ore, 1>,)>,
    {
        fn on_each_tick<'tick>(&mut self, tick: &PastTick<'tick>) {
            if let Some(territory) = &mut self.territory {
                let outputs = territory.past_resources(tick);
                self.furnace.past_inputs(tick).0.add(outputs.empty());
            }
        }
    }

    /// A pair of a territory and a furnace that smelts the ore from that territory.
    pub struct SmeltedOreFactory<Ore, R>(Subfactory<SmeltedOreFactoryInner<Ore, R>>)
    where
        Ore: OreType,
        R: FurnaceRecipe<InputBundle = (Bundle<Ore, 1>,)>;

    impl<O, R, S> SmeltedOreFactory<O, R>
    where
        O: OreType,
        S: ResourceType,
        R: FurnaceRecipe<
                InputBundle = (Bundle<O, 1>,),
                InputResources = (Resource<O>,),
                OutputBundle = (Bundle<S, 1>,),
                OutputResources = (Resource<S>,),
            >,
    {
        pub const fn new(tick: &Tick, territory: Territory<O>, furnace: Furnace<R>) -> Self {
            SmeltedOreFactory(Subfactory::new(
                tick,
                SmeltedOreFactoryInner {
                    territory: Some(territory),
                    furnace,
                },
            ))
        }
        pub fn hand_mine(&mut self, tick: &mut Tick) {
            let mut territory = self.0.inner(tick).territory.take().unwrap();
            let ore = territory.hand_mine::<1>(tick);
            self.0.inner(tick).territory = Some(territory);
            self.0.inner(tick).furnace.inputs(tick).0.add(ore);
        }
        pub fn num_miners(&mut self, tick: &Tick) -> u32 {
            self.0.inner(tick).territory.as_ref().unwrap().num_miners()
        }
        pub fn add_miner(&mut self, tick: &Tick, miner: Miner) {
            self.0
                .inner(tick)
                .territory
                .as_mut()
                .unwrap()
                .add_miner(tick, miner)
                .unwrap();
        }
        /// Count the number of ore items being processed by the furnace.
        pub fn in_process_count(&mut self, tick: &Tick) -> u32 {
            self.0.inner(tick).furnace.inputs(tick).0.amount()
        }
        pub fn outputs<'a, 'tick>(&'a mut self, tick: &'a Tick) -> &'a mut Resource<S> {
            &mut self.0.inner(tick).furnace.outputs(tick).0
        }
        pub fn past_outputs<'a, 'tick>(
            &'a mut self,
            tick: &'a PastTick<'tick>,
        ) -> &'a mut Past<'tick, Resource<S>> {
            &mut self.0.past_inner(tick).furnace.past_outputs(tick).0
        }
    }
}

use copper_wire_factory::CopperWireFactory;
mod copper_wire_factory {
    use rustorio::{
        Resource, Tick,
        buildings::Assembler,
        recipes::{CopperSmelting, CopperWireRecipe},
        resources::{CopperOre, CopperWire},
    };
    use rustorio_engine::subfactories::{OnEachTick, Past, PastTick, Subfactory};

    use crate::smelted_ore_factory::SmeltedOreFactory;

    struct CopperWireFactoryInner {
        copper: SmeltedOreFactory<CopperOre, CopperSmelting>,
        assembler: Assembler<CopperWireRecipe>,
    }

    impl OnEachTick for CopperWireFactoryInner {
        fn on_each_tick<'tick>(&mut self, tick: &PastTick<'tick>) {
            let outputs = self.copper.past_outputs(tick);
            self.assembler.past_inputs(tick).0.add(outputs.empty());
        }
    }

    pub struct CopperWireFactory(Subfactory<CopperWireFactoryInner>);

    impl CopperWireFactory {
        pub const fn new(
            tick: &Tick,
            copper: SmeltedOreFactory<CopperOre, CopperSmelting>,
            assembler: Assembler<CopperWireRecipe>,
        ) -> Self {
            CopperWireFactory(Subfactory::new(
                tick,
                CopperWireFactoryInner { copper, assembler },
            ))
        }
        pub fn into_inner(
            self,
            tick: &Tick,
        ) -> (
            SmeltedOreFactory<CopperOre, CopperSmelting>,
            Assembler<CopperWireRecipe>,
        ) {
            let inner = self.0.into_inner(tick);
            (inner.copper, inner.assembler)
        }
        pub fn outputs<'a, 'tick>(&'a mut self, tick: &'a Tick) -> &'a mut Resource<CopperWire> {
            &mut self.0.inner(tick).assembler.outputs(tick).0
        }
        pub fn past_outputs<'a, 'tick>(
            &'a mut self,
            tick: &'a PastTick<'tick>,
        ) -> &'a mut Past<'tick, Resource<CopperWire>> {
            &mut self.0.past_inner(tick).assembler.past_outputs(tick).0
        }
    }
}

use assembler_factory::AssemblerFactory;
mod assembler_factory {
    use rustorio::{
        Tick,
        buildings::Assembler,
        recipes::{AssemblerRecipe, IronSmelting},
        resources::IronOre,
    };

    use crate::{copper_wire_factory::CopperWireFactory, smelted_ore_factory::SmeltedOreFactory};

    pub struct AssemblerFactory {
        iron: SmeltedOreFactory<IronOre, IronSmelting>,
        copper_wire: CopperWireFactory,
    }

    impl AssemblerFactory {
        pub const fn new(
            _tick: &Tick,
            iron: SmeltedOreFactory<IronOre, IronSmelting>,
            copper_wire: CopperWireFactory,
        ) -> Self {
            AssemblerFactory { iron, copper_wire }
        }
        pub fn into_inner(
            self,
            _tick: &Tick,
        ) -> (SmeltedOreFactory<IronOre, IronSmelting>, CopperWireFactory) {
            (self.iron, self.copper_wire)
        }
        pub fn try_new_assembler<R: AssemblerRecipe>(
            &mut self,
            tick: &Tick,
            r: R,
        ) -> Option<Assembler<R>> {
            if self.iron.outputs(tick).amount() >= 6
                && self.copper_wire.outputs(tick).amount() >= 12
            {
                Some(Assembler::build(
                    &tick,
                    r,
                    self.copper_wire.outputs(tick).bundle().unwrap(),
                    self.iron.outputs(tick).bundle().unwrap(),
                ))
            } else {
                None
            }
        }
        pub fn new_assembler<R: AssemblerRecipe + Copy>(
            &mut self,
            tick: &mut Tick,
            r: R,
        ) -> Assembler<R> {
            loop {
                match self.try_new_assembler(&tick, r) {
                    Some(assembler) => break assembler,
                    None => tick.advance(),
                }
            }
        }
    }
}

use science_factory::{ScienceFactory, ScienceFactoryInner};
mod science_factory {
    use rustorio::{
        Bundle, Recipe, Resource, TechRecipe, Technology, Tick,
        buildings::{Assembler, Lab},
        recipes::{ElectronicCircuitRecipe, IronSmelting, RedScienceRecipe},
        research::RedScience,
        resources::IronOre,
    };
    use rustorio_engine::subfactories::{OnEachTick, PastTick, Subfactory};

    use crate::{copper_wire_factory::CopperWireFactory, smelted_ore_factory::SmeltedOreFactory};

    pub struct ScienceFactoryInner<T: Technology> {
        pub iron_factory: SmeltedOreFactory<IronOre, IronSmelting>,
        pub copper_wire_factory: CopperWireFactory,
        pub circuit_assembler: Assembler<ElectronicCircuitRecipe>,
        pub red_science_assembler: Assembler<RedScienceRecipe>,
        pub lab: Lab<T>,
    }

    impl<T: Technology> OnEachTick for ScienceFactoryInner<T>
    where
        TechRecipe<T>: Recipe<InputBundle = (Bundle<RedScience, 1>,), InputResources = (Resource<RedScience>,)>,
    {
        fn on_each_tick<'tick>(&mut self, tick: &PastTick<'tick>) {
            self.lab.past_inputs(tick).0 += self.red_science_assembler.past_outputs(tick).0.empty();

            let iron = self
                .iron_factory
                .past_outputs(tick)
                .split_off_max(5 - self.red_science_assembler.past_inputs(tick).0.amount());
            self.red_science_assembler.past_inputs(tick).0 += iron;
            self.red_science_assembler.past_inputs(tick).1 +=
                self.circuit_assembler.past_outputs(tick).0.empty();

            self.circuit_assembler.past_inputs(tick).0 +=
                self.iron_factory.past_outputs(tick).empty();
            self.circuit_assembler.past_inputs(tick).1 +=
                self.copper_wire_factory.past_outputs(tick).empty();
        }
    }

    pub struct ScienceFactory<T: Technology>(Subfactory<ScienceFactoryInner<T>>)
    where
        TechRecipe<T>: Recipe<InputBundle = (Bundle<RedScience, 1>,), InputResources = (Resource<RedScience>,)>;

    impl<T: Technology> ScienceFactory<T>
    where
        TechRecipe<T>: Recipe<InputBundle = (Bundle<RedScience, 1>,), InputResources = (Resource<RedScience>,)>,
    {
        pub const fn new(
            tick: &Tick,
            iron_factory: SmeltedOreFactory<IronOre, IronSmelting>,
            copper_wire_factory: CopperWireFactory,
            circuit_assembler: Assembler<ElectronicCircuitRecipe>,
            science_assembler: Assembler<RedScienceRecipe>,
            lab: Lab<T>,
        ) -> Self {
            ScienceFactory(Subfactory::new(
                tick,
                ScienceFactoryInner {
                    iron_factory,
                    copper_wire_factory,
                    circuit_assembler,
                    red_science_assembler: science_assembler,
                    lab,
                },
            ))
        }
        pub fn into_inner(self, tick: &Tick) -> ScienceFactoryInner<T> {
            self.0.into_inner(tick)
        }
        pub fn change_technology<T2: Technology>(
            self,
            tick: &Tick,
            technology: &T2,
        ) -> ScienceFactory<T2>
        where
            TechRecipe<T2>: Recipe<
                    InputBundle = (Bundle<RedScience, 1>,),
                    InputResources = (Resource<RedScience>,),
                >,
        {
            let ScienceFactoryInner {
                iron_factory,
                copper_wire_factory,
                circuit_assembler,
                red_science_assembler,
                lab,
            } = self.into_inner(tick);
            let lab = lab.change_technology(technology).unwrap();
            ScienceFactory::new(
                &tick,
                iron_factory,
                copper_wire_factory,
                circuit_assembler,
                red_science_assembler,
                lab,
            )
        }

        pub fn outputs<'a, 'tick>(
            &'a mut self,
            tick: &'a Tick,
        ) -> &'a mut <TechRecipe<T> as Recipe>::OutputResources {
            self.0.inner(tick).lab.outputs(tick)
        }
        // pub fn past_outputs<'a, 'tick>(
        //     &'a mut self,
        //     tick: &'a PastTick<'tick>,
        // ) -> &'a mut Past<'tick, Resource<CopperWire>> {
        //     &mut self.0.past_inner(tick).lab.past_outputs(tick).0
        // }
    }
}

fn hand_craft<R: HandRecipe>(inputs: R::InputBundle, tick: &mut Tick) -> R::OutputBundle {
    R::craft(tick, inputs)
}

fn user_main(mut tick: Tick, starting_resources: StartingResources) -> (Tick, VictoryResources) {
    let StartingResources {
        iron,
        iron_territory,
        copper_territory,
        steel_technology,
    } = starting_resources;

    let iron_furnace = Furnace::build(&tick, IronSmelting, iron);
    let mut iron_factory = SmeltedOreFactory::new(&tick, iron_territory, iron_furnace);

    let copper_furnace = loop {
        iron_factory.hand_mine(&mut tick);
        if let Ok(iron) = iron_factory.outputs(&tick).bundle() {
            break Furnace::build(&tick, CopperSmelting, iron);
        }
    };
    let mut copper_factory = SmeltedOreFactory::new(&tick, copper_territory, copper_furnace);

    println!(
        "Created additional furnace for copper at tick {}",
        tick.cur()
    );

    while iron_factory.num_miners(&tick) < 3 || copper_factory.num_miners(&tick) < 3 {
        if copper_factory.in_process_count(&tick) < iron_factory.in_process_count(&tick) {
            copper_factory.hand_mine(&mut tick);
        } else {
            iron_factory.hand_mine(&mut tick);
        }
        if iron_factory.outputs(&tick).amount() >= 10 && copper_factory.outputs(&tick).amount() >= 5
        {
            let iron = iron_factory.outputs(&tick).bundle().unwrap();
            let copper = copper_factory.outputs(&tick).bundle().unwrap();
            let miner = Miner::build(iron, copper);
            if iron_factory.num_miners(&tick) < 3 {
                iron_factory.add_miner(&tick, miner);
            } else {
                copper_factory.add_miner(&tick, miner);
            }
        }
    }

    let mut copper_wire: Resource<CopperWire> = Resource::new_empty();
    let copper_wire_assembler = loop {
        if let Ok(copper) = copper_factory.outputs(&tick).bundle() {
            copper_wire += hand_craft::<CopperWireRecipe>((copper,), &mut tick).0;
        } else {
            tick.advance();
        }
        if copper_wire.amount() >= 12 && iron_factory.outputs(&tick).amount() >= 6 {
            break Assembler::build(
                &tick,
                CopperWireRecipe,
                copper_wire.bundle().unwrap(),
                iron_factory.outputs(&tick).bundle().unwrap(),
            );
        }
    };
    let copper_wire_factory = CopperWireFactory::new(&tick, copper_factory, copper_wire_assembler);
    println!("Created assembler for copper wire at tick {}", tick.cur());

    let mut assembler_factory = AssemblerFactory::new(&tick, iron_factory, copper_wire_factory);

    let circuit_assembler = assembler_factory.new_assembler(&mut tick, ElectronicCircuitRecipe);
    println!("Created assembler for circuits at tick {}", tick.cur());

    let red_science_assembler = assembler_factory.new_assembler(&mut tick, RedScienceRecipe);
    println!("Created assembler for red science at tick {}", tick.cur());

    let (mut iron_factory, copper_wire_factory) = assembler_factory.into_inner(&tick);
    let (mut copper_factory, copper_wire_assembler) = copper_wire_factory.into_inner(&tick);

    let lab = loop {
        if copper_factory.in_process_count(&tick) < iron_factory.in_process_count(&tick) {
            copper_factory.hand_mine(&mut tick);
        } else {
            iron_factory.hand_mine(&mut tick);
        }
        if iron_factory.outputs(&tick).amount() >= 20
            && let Ok(copper) = copper_factory.outputs(&tick).bundle()
        {
            break Lab::build(
                &tick,
                &steel_technology,
                iron_factory.outputs(&tick).bundle().unwrap(),
                copper,
            );
        }
    };
    println!("Created lab at tick {}", tick.cur());

    let copper_wire_factory = CopperWireFactory::new(&tick, copper_factory, copper_wire_assembler);
    let mut science_factory = ScienceFactory::new(
        &tick,
        iron_factory,
        copper_wire_factory,
        circuit_assembler,
        red_science_assembler,
        lab,
    );

    let (steel_smelting, points_technology) = loop {
        match science_factory.outputs(&tick).bundle() {
            Ok(bundle) => break steel_technology.research(bundle),
            Err(_) => tick.advance(),
        }
    };

    println!("Researched steel technology at tick {}", tick.cur());

    let mut science_factory = science_factory.change_technology(&tick, &points_technology);

    let point_recipe = loop {
        match science_factory.outputs(&tick).bundle() {
            Ok(bundle) => break points_technology.research(bundle),
            Err(_) => tick.advance(),
        }
    };

    let ScienceFactoryInner {
        mut iron_factory,
        copper_wire_factory,
        mut circuit_assembler,
        red_science_assembler: _,
        lab: _,
    } = science_factory.into_inner(&tick);

    println!("Researched points technology at tick {}", tick.cur());

    let mut steel_furnace = loop {
        match iron_factory.outputs(&tick).bundle() {
            Ok(bundle) => break Furnace::build(&tick, steel_smelting, bundle),
            Err(_) => tick.advance(),
        }
    };

    println!("Created furnace for steel at tick {}", tick.cur());

    let mut assembler_factory = AssemblerFactory::new(&tick, iron_factory, copper_wire_factory);
    let mut point_assembler = assembler_factory.new_assembler(&mut tick, point_recipe);
    let (mut iron_factory, mut copper_wire_factory) = assembler_factory.into_inner(&tick);

    println!("Created assembler for points at tick {}", tick.cur());

    let mut points: Resource<Point> = Resource::new_empty();
    while points.amount() < 200 {
        points += point_assembler.outputs(&tick).0.empty();

        point_assembler.inputs(&tick).0 += circuit_assembler.outputs(&tick).0.empty();
        point_assembler.inputs(&tick).1 += steel_furnace.outputs(&tick).0.empty();

        let mut iron_output = iron_factory.outputs(&tick).empty();
        // Prioritize steel furnace since it needs 5 iron per steel
        let iron_for_steel = iron_output.split_off_max(5 - steel_furnace.inputs(&tick).0.amount());
        steel_furnace.inputs(&tick).0 += iron_for_steel;
        // Rest goes to circuit assembler
        circuit_assembler.inputs(&tick).0 += iron_output;

        circuit_assembler.inputs(&tick).1 += copper_wire_factory.outputs(&tick).empty();

        tick.advance();
    }

    println!("Produced 200 points at tick {}", tick.cur());

    assert_eq!(tick.cur(), 12449);
    (tick, points.bundle().unwrap())
}
