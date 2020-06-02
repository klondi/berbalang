use std::cmp::{Ord, PartialOrd};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indexmap::map::IndexMap;
use indexmap::set::IndexSet;
use prefix_tree::PrefixSet;
use seahash::{hash, hash_seeded};
use serde::Deserialize;
use unicorn::Cpu;

use crate::emulator::executor::Register;
use crate::error::Error;
use byteorder::{ByteOrder, LittleEndian};
use std::convert::TryFrom;

// TODO: why store the size at all, if you're just going to
// throw it away?
#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Block {
    pub entry: u64,
    pub size: usize,
    //pub code: Vec<u8>,
}

impl fmt::Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[BLOCK 0x{:08x} - 0x{:08x}]",
            self.entry,
            self.entry + self.size as u64
        )
    }
}

pub struct Profiler<C: Cpu<'static>> {
    /// The Arc<Mutex<_>> fields need to be writeable for the unicorn callbacks.
    pub block_log: Arc<Mutex<Vec<Block>>>,
    /// These fields are written to after the emulation has finished.
    pub cpu_error: Option<unicorn::Error>,
    pub computation_time: Duration,
    pub registers: IndexMap<Register<C>, u64>,
    registers_to_read: Vec<Register<C>>,
}

impl<C: Cpu<'static>> Profiler<C> {
    pub fn strong_counts(&self) -> usize {
        Arc::strong_count(&self.block_log)
    }
}

fn convert_register_map<C: Cpu<'static>>(registers: RegisterPattern<C>) -> RegisterPatternConfig {
    let mut map = IndexMap::new();
    for (k, v) in registers.0.into_iter() {
        map.insert(format!("{:?}", k), v); // FIXME use stable conversion method
    }
    RegisterPatternConfig(map)
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub paths: PrefixSet<Block>,
    pub cpu_errors: IndexMap<unicorn::Error, usize>,
    pub computation_times: Vec<Duration>,
    pub registers: Vec<RegisterPatternConfig>,
}

impl Profile {
    pub fn collate<C: 'static + Cpu<'static>>(profilers: Vec<Profiler<C>>) -> Self {
        //let mut write_trie = Trie::new();
        let mut paths = PrefixSet::new();
        let mut cpu_errors = IndexMap::new();
        let mut computation_times = Vec::new();
        let mut register_maps = Vec::new();

        for Profiler {
            block_log,
            //write_log,
            cpu_error,
            computation_time,
            registers,
            ..
        } in profilers.into_iter()
        {
            paths.insert::<Vec<Block>>(
                (*block_log.lock().unwrap())
                    .iter()
                    .map(Clone::clone)
                    //.map(|b| (b.entry, b.size))
                    .collect::<Vec<Block>>(),
            );
            //write_trie.insert((*write_log.lock().unwrap()).clone(), ());
            if let Some(c) = cpu_error {
                *cpu_errors.entry(c).or_insert(0) += 1;
            };
            computation_times.push(computation_time);
            register_maps.push(convert_register_map::<C>(RegisterPattern(registers)));
        }

        Self {
            paths,
            //write_trie,
            cpu_errors,
            computation_times,
            registers: register_maps,
        }
    }
}

impl<C: 'static + Cpu<'static>> From<Vec<Profiler<C>>> for Profile {
    fn from(v: Vec<Profiler<C>>) -> Self {
        Self::collate(v)
    }
}

impl<C: Cpu<'static>> fmt::Debug for Profiler<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // write!(
        //     f,
        //     "write_log: {} entries; ",
        //     self.write_log.lock().unwrap().len()
        // )?;
        write!(f, "registers: {:?}; ", self.registers)?;
        write!(f, "cpu_error: {:?}; ", self.cpu_error)?;
        write!(
            f,
            "computation_time: {} μs; ",
            self.computation_time.as_micros()
        )?;
        write!(f, "{} blocks", self.block_log.lock().unwrap().len())
    }
}

impl<C: Cpu<'static>> Profiler<C> {
    pub fn new(output_registers: &[Register<C>]) -> Self {
        Self {
            registers_to_read: output_registers.to_vec(),
            ..Default::default()
        }
    }

    pub fn read_registers(&mut self, emu: &mut C) {
        for r in &self.registers_to_read {
            let val = emu.reg_read(*r).expect("Failed to read register!");
            self.registers.insert(*r, val);
        }
    }

    pub fn register(&self, reg: Register<C>) -> Option<u64> {
        self.registers.get(&reg).cloned()
    }

    pub fn set_error(&mut self, error: unicorn::Error) {
        self.cpu_error = Some(error)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct MemLogEntry {
    pub program_counter: u64,
    pub mem_address: u64,
    pub num_bytes_written: usize,
    pub value: i64,
}

impl<C: Cpu<'static>> Default for Profiler<C> {
    fn default() -> Self {
        Self {
            //write_log: Arc::new(Mutex::new(Vec::default())),
            registers: IndexMap::default(),
            cpu_error: None,
            registers_to_read: Vec::new(),
            computation_time: Duration::default(),
            block_log: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[cfg(test)]
mod test {
    use unicorn::CpuX86;

    use super::*;

    #[test]
    fn test_collate() {
        let profilers: Vec<Profiler<CpuX86<'_>>> = vec![
            Profiler {
                block_log: Arc::new(Mutex::new(vec![
                    Block { entry: 1, size: 2 },
                    Block { entry: 3, size: 4 },
                ])),
                cpu_error: None,
                computation_time: Default::default(),
                registers: IndexMap::new(),
                registers_to_read: vec![],
            },
            Profiler {
                block_log: Arc::new(Mutex::new(vec![
                    Block { entry: 1, size: 2 },
                    Block { entry: 6, size: 6 },
                ])),
                cpu_error: None,
                computation_time: Default::default(),
                registers: IndexMap::new(),
                registers_to_read: vec![],
            },
        ];

        let profile: Profile = profilers.into();

        println!("{:#?}", profile);
        println!(
            "size of profile in mem: {}",
            std::mem::size_of_val(&profile.paths)
        );
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct RegisterPatternConfig(pub IndexMap<String, u64>);

#[derive(Debug)]
pub struct RegisterPattern<C: 'static + Cpu<'static>>(pub IndexMap<Register<C>, u64>);

impl<C: 'static + Cpu<'static>> TryFrom<&RegisterPatternConfig> for RegisterPattern<C> {
    type Error = Error;

    fn try_from(rp: &RegisterPatternConfig) -> Result<Self, Self::Error> {
        let mut map = IndexMap::new();
        for (k, v) in rp.0.iter() {
            let reg = k
                .parse()
                .map_err(|_| Self::Error::Parsing("Failed to parse register string".to_string()))?;
            map.insert(reg, *v);
        }
        Ok(RegisterPattern(map))
    }
}

fn byte_positions(bytes: &[u8], grain: usize) -> Vec<[u8; 4]> {
    let len = bytes.len();
    debug_assert!(grain < len);
    let chunk = len / grain;
    bytes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let (byte, pos) = (*b, i / chunk);
            let mut buf = [0_u8; 4];
            buf[0] = byte;
            buf[1] = (pos & 0xFF) as u8;
            buf[2] = ((pos >> 8) & 0xFF) as u8;
            buf[3] = ((pos >> 16) & 0xFF) as u8;
            buf
        })
        .collect::<Vec<[u8; 4]>>()
}

impl RegisterPatternConfig {
    /// See https://en.wikipedia.org/wiki/MinHash for discussion of algorithm
    fn jaccard(&self, other: &Self, grain: usize, num_hashes: u64) -> f64 {
        // the keys in the profile's maps and the pattern's map
        // should be in an identical order, just because nothing should
        // have disturbed them. But it would be better to verify this.
        let self_bytes: Vec<u8> = self.into();
        let other_bytes: Vec<u8> = other.into();
        let self_byte_pos = byte_positions(&self_bytes, grain);
        let other_byte_pos = byte_positions(&other_bytes, grain);

        (0_u64..num_hashes)
            .filter(|seed| {
                let s = self_byte_pos
                    .iter()
                    .map(|b| hash_seeded(b, *seed, 0, 0, 0))
                    .min();
                let o = other_byte_pos
                    .iter()
                    .map(|b| hash_seeded(b, *seed, 0, 0, 0))
                    .min();
                s == o
            })
            .count() as f64
            / num_hashes as f64
    }
}

impl From<&RegisterPatternConfig> for Vec<u8> {
    fn from(rp: &RegisterPatternConfig) -> Vec<u8> {
        const WORD_SIZE: usize = 8; // FIXME
        let len = rp.0.keys().len();
        let mut buf = vec![0_u8; len * WORD_SIZE];
        let mut offset = 0;
        for word in rp.0.values() {
            LittleEndian::write_u64(&mut buf[offset..], *word);
            offset += WORD_SIZE;
        }
        buf
    }
}
