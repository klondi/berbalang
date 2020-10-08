use std::cmp::Ordering;
use std::fmt::Debug;
use std::path::Path;

use chrono::prelude::*;
use hashbrown::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

use crate::emulator::register_pattern::{parse_register_pattern_file, RegisterPattern};
use crate::error::Error;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DataConfig {
    pub path: String,
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub enum Job {
    Roper,
    Hello,
    LinearGp,
}

impl Default for Job {
    fn default() -> Self {
        Self::Roper
    }
}

fn default_num_islands() -> usize {
    num_cpus::get() / 4
}

fn default_random_seed() -> u64 {
    rand::random::<u64>()
}

fn default_one() -> f64 {
    1.0
}

fn default_crossover_algorithm() -> String {
    "alternating".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct Config {
    pub job: Job,
    pub timeout: Option<String>,
    pub selection: Selection,
    #[serde(default = "default_num_islands")]
    pub num_islands: usize,
    // The island identifier is used internally
    #[serde(default)]
    pub island_id: usize,
    pub crossover_period: f64,
    #[serde(default = "default_crossover_algorithm")]
    pub crossover_algorithm: String,
    pub crossover_rate: f64,
    #[serde(default)]
    pub data: DataConfig,
    pub max_init_len: usize,
    pub max_length: usize,
    pub min_init_len: usize,
    // See the comments in util::levy_flight for an explanation
    // There is a mutation_rate chance, per genome, that
    // a levy-flight pointwise decision process will be applied, per-gene,
    // using mutation_exponent as its lambda parameter.
    #[serde(default = "default_one")]
    pub mutation_rate: f64,
    pub mutation_exponent: f64,
    pub observer: ObserverConfig,
    pub pop_size: usize,
    pub problems: Option<Vec<ClassificationProblem>>,
    #[serde(default)]
    pub roulette: RouletteConfig,
    #[serde(default)]
    pub tournament: TournamentConfig,
    #[serde(default)]
    pub roper: RoperConfig,
    #[serde(default)]
    pub linear_gp: LinearGpConfig,
    #[serde(default)]
    pub hello: HelloConfig,
    pub num_epochs: usize,
    pub fitness: FitnessConfig,
    #[serde(default = "default_random_seed")]
    pub random_seed: u64,
    #[serde(default)]
    pub push_vm: PushVm,
}

fn default_tournament_size() -> usize {
    4
}

fn default_push_vm_max_steps() -> usize {
    0x1000
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PushVm {
    #[serde(default = "default_push_vm_max_steps")]
    pub max_steps: usize,
    pub min_len: usize,
    pub max_len: usize,
    pub literal_rate: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FitnessConfig {
    pub target: f64,
    pub eval_by_case: bool,
    pub dynamic: bool,
    #[serde(default)]
    priority: String,
    pub function: String,
    pub weighting: String,
}

impl FitnessConfig {
    pub fn priority(&self) -> &str {
        if self.priority.is_empty() {
            &self.weighting
        } else {
            &self.priority
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TournamentConfig {
    #[serde(default = "default_tournament_size")]
    pub tournament_size: usize,
    pub geographic_radius: usize,
    pub migration_rate: f64,
    pub num_offspring: usize,
    pub num_parents: usize,
}

fn default_weight_decay() -> f64 {
    0.75
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RouletteConfig {
    #[serde(default = "default_weight_decay")]
    pub weight_decay: f64,
}

fn random_population_name() -> String {
    // we're letting this random value be unseeded for now, since
    // the name impacts nothing and we don't want to clobber same-seeded runs
    let seed = rand::random::<u64>();
    crate::util::name::random(2, seed)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObserverConfig {
    pub dump_population: bool,
    pub dump_soup: bool,
    #[serde(default)]
    pub full_data_directory: String,
    data_directory: String,
    #[serde(default = "random_population_name")]
    pub population_name: String,
}

impl Config {
    pub fn epoch_length(&self) -> usize {
        self.pop_size / self.tournament.num_offspring
    }

    pub fn assert_invariants(&self) {
        assert!(self.tournament.tournament_size >= self.tournament.num_offspring + 2);
        //assert_eq!(self.num_offspring, 2); // all that's supported for now
    }

    pub fn from_path<P: AsRef<Path>>(
        path: P,
        population_name: Option<String>,
    ) -> Result<Self, Error> {
        let mut config: Self = toml::from_str(&std::fs::read_to_string(&path)?)?;
        if let Some(population_name) = population_name {
            config.observer.population_name = population_name;
        }
        // add the hostname
        config.observer.population_name = format!(
            "{}-{}",
            gethostname::gethostname()
                .to_str()
                .expect("Failed to get hostname"),
            config.observer.population_name
        );
        config.assert_invariants();
        config.set_data_directory();
        // copy the config file to the data directory for posterity
        // bit ugly, here: copying it to the parent of the directory, just above the island subdirs
        std::fs::copy(
            &path,
            &format!("{}/../config.toml", config.data_directory()),
        )?;

        println!("{:#?}", config);

        Ok(config)
    }

    /// Returns the path to the full data directory, creating it if necessary.
    pub fn set_data_directory(&mut self) {
        let local_date: DateTime<Local> = Local::now();

        let mut data_dir = self.observer.data_directory.clone();
        if data_dir.starts_with('~') {
            let home = std::env::var("HOME")
                .expect("No HOME environment variable found. Please set this.");
            data_dir.replace_range(0..1, &home);
        };

        let path = format!(
            "{data_dir}/berbalang/{job:?}/{selection:?}/{date}/{pop_name}/island_{island}",
            data_dir = data_dir,
            job = self.job,
            selection = self.selection,
            date = local_date.format("%Y/%m/%d"),
            pop_name = self.observer.population_name,
            island = self.island_id,
        );

        for sub in ["", "soup", "population", "champions"].iter() {
            let d = format!("{}/{}", path, sub);
            std::fs::create_dir_all(&d)
                .map_err(|e| {
                    log::error!("Error creating {}: {:?}", path, e);
                    e
                })
                .expect("Failed to create data directory");
        }

        self.observer.full_data_directory = path;
    }

    pub fn data_directory(&self) -> &str {
        &self.observer.full_data_directory
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct HelloConfig {
    pub target: String,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct LinearGpConfig {
    pub max_steps: usize,
    // NOTE: these register values will be overridden if a data
    // file has been supplied, in order to accommodate that data
    pub num_registers: Option<usize>,
    pub return_registers: Option<usize>,
}

fn default_arch() -> unicorn::Arch {
    unicorn::Arch::X86
}

fn default_mode() -> unicorn::Mode {
    unicorn::Mode::MODE_32
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct RoperConfig {
    #[serde(default)]
    pub use_push: bool,
    pub gadget_file: Option<String>,
    #[serde(default)]
    pub output_registers: Vec<String>,
    #[serde(default)]
    pub input_registers: Vec<String>,
    #[serde(default)]
    pub randomize_registers: bool,
    pub register_pattern_file: Option<String>,
    #[serde(skip)]
    pub parsed_register_patterns: Vec<RegisterPattern>,
    #[serde(default = "Default::default")]
    pub soup: Option<Vec<u64>>,
    pub soup_size: Option<usize>,
    // if no gadget file given
    #[serde(default = "default_arch")]
    pub arch: unicorn::Arch,
    #[serde(default = "default_mode")]
    pub mode: unicorn::Mode,
    #[serde(default = "default_num_workers")]
    pub num_workers: usize,
    #[serde(default = "default_num_emu")]
    pub num_emulators: usize,
    #[serde(default = "default_wait_limit")]
    pub wait_limit: u64,
    pub max_emu_steps: Option<usize>,
    pub millisecond_timeout: Option<u64>,
    #[serde(default = "Default::default")]
    pub record_basic_blocks: bool,
    #[serde(default = "Default::default")]
    pub record_memory_writes: bool,
    #[serde(default = "default_stack_size")]
    pub emulator_stack_size: usize,
    pub binary_path: String,
    #[serde(default)]
    pub ld_paths: Option<Vec<String>>,
    #[serde(default)]
    pub bad_bytes: Option<HashMap<String, u8>>,
    pub memory_pattern: Option<Vec<u8>>,
    #[serde(default)]
    pub break_on_calls: bool,
    #[serde(default)]
    pub monitor_stack_writes: bool,
}

impl RoperConfig {
    pub fn parse_register_patterns(&mut self) {
        if let Some(ref pat_file) = self.register_pattern_file {
            let ps = parse_register_pattern_file(pat_file)
                .expect("Failed to parse register pattern file");
            log::info!("Parsed and reduced register patterns: {:#x?}", ps);
            self.parsed_register_patterns = ps;
        }
    }

    pub fn register_patterns(&self) -> &[RegisterPattern] {
        &self.parsed_register_patterns
    }

    pub fn registers_to_check(&self) -> Vec<String> {
        let mut set = HashSet::new();
        for r in self
            .output_registers
            .iter()
            .chain(self.input_registers.iter())
        {
            set.insert(r.clone());
        }
        for rp in self.parsed_register_patterns.iter() {
            for r in rp.0.keys() {
                set.insert(r.clone());
            }
        }
        set.into_iter().collect::<Vec<String>>()
    }
}

fn default_num_workers() -> usize {
    num_cpus::get()
}

fn default_num_emu() -> usize {
    default_num_workers() + 1
}

const fn default_wait_limit() -> u64 {
    200
}

const fn default_stack_size() -> usize {
    0x1000
}

impl Default for RoperConfig {
    fn default() -> Self {
        Self {
            use_push: false,
            gadget_file: None,
            output_registers: vec![],
            input_registers: vec![],
            randomize_registers: false,
            register_pattern_file: None,
            parsed_register_patterns: vec![],
            soup: None,
            soup_size: None,
            arch: unicorn::Arch::X86,
            mode: unicorn::Mode::MODE_64,
            memory_pattern: None,
            num_workers: 8,
            num_emulators: 8,
            wait_limit: 500,
            max_emu_steps: Some(0x10_000),
            millisecond_timeout: Some(500),
            record_basic_blocks: false,
            record_memory_writes: false,
            emulator_stack_size: 0x1000,
            binary_path: "/bin/sh".to_string(),
            ld_paths: None,
            bad_bytes: None,
            break_on_calls: false,
            monitor_stack_writes: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct ClassificationProblem {
    pub input: Vec<i32>,
    // TODO make this more generic
    pub output: i32,
    // Ditto
    pub tag: u64,
}

impl PartialOrd for ClassificationProblem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.tag.partial_cmp(&other.tag)
    }
}

impl Ord for ClassificationProblem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tag.cmp(&other.tag)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Selection {
    Tournament,
    Roulette,
    Metropolis,
    Lexicase,
}

impl Default for Selection {
    fn default() -> Self {
        Self::Tournament
    }
}

#[derive(Clone, Debug, Serialize)]
pub enum Problem {
    Classification(ClassificationProblem),
    RegisterSpecification(RegisterPattern),
    MemoryPattern(Vec<u8>),
}
