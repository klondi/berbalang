use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::ops::Index;
use std::sync::Mutex;

use itertools::Itertools;
use serde::export::Formatter;
use serde::{Deserialize, Serialize};

pub type FitnessMap<'a> = BTreeMap<&'a str, f64>;

pub trait HasScalar {
    fn scalar(&self) -> f64;
}

impl HasScalar for Vec<f64> {
    fn scalar(&self) -> f64 {
        self.iter().sum()
    }
}

pub trait FitnessScore:
    Sized + PartialEq + Debug + Send + Clone + PartialOrd + Serialize + PartialOrd + HasScalar
{
}

impl FitnessScore for Vec<f64> {}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pareto<'a>(#[serde(borrow)] BTreeMap<&'a str, f64>);

impl Pareto<'static> {
    pub fn new() -> Self {
        Pareto(BTreeMap::new())
    }

    pub fn values(&self) -> impl Iterator<Item = &f64> {
        self.inner().iter().sorted_by_key(|p| p.0).map(|(_k, v)| v)
    }
}

impl HasScalar for Pareto<'static> {
    fn scalar(&self) -> f64 {
        self.values().sum()
    }
}

impl MapFit for Pareto<'static> {
    fn inner_mut(&mut self) -> &mut BTreeMap<&'static str, f64> {
        &mut self.0
    }

    fn inner(&self) -> &BTreeMap<&'static str, f64> {
        &self.0
    }

    fn from_map(map: BTreeMap<&'static str, f64>) -> Self {
        Self(map)
    }
}

pub trait MapFit {
    fn inner_mut(&mut self) -> &mut BTreeMap<&'static str, f64>;

    fn inner(&self) -> &BTreeMap<&'static str, f64>;

    fn from_map(map: BTreeMap<&'static str, f64>) -> Self
    where
        Self: Sized;

    fn insert(&mut self, name: &'static str, thing: f64) {
        self.inner_mut().insert(name, thing);
    }

    fn get(&self, name: &str) -> Option<f64> {
        (self.inner().get(name)).cloned()
    }

    fn average(frame: &[&Self]) -> Self
    where
        Self: Sized,
    {
        let mut map = FitnessMap::new();
        for p in frame.iter() {
            for (&k, &v) in p.inner().iter() {
                *(map.entry(k).or_insert(0.0)) += v;
            }
        }
        let len = frame.len() as f64;
        for (_k, v) in map.iter_mut() {
            *v /= len;
        }
        Self::from_map(map)
    }
}

impl FitnessScore for Pareto<'static> {}

impl PartialOrd for Pareto<'static> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        debug_assert_eq!(
            self.0.len(),
            other.0.len(),
            "vectors must have the same length in order to perform Pareto comparisons"
        );
        if self < other {
            Some(Ordering::Less)
        } else if other < self {
            Some(Ordering::Greater)
        } else {
            None
        }
    }

    fn lt(&self, other: &Self) -> bool {
        assert_eq!(
            self.0.len(),
            other.0.len(),
            "vectors must have the same length in order to perform Pareto comparisons"
        );
        self.values().zip(other.values()).all(|(x, y)| x <= y)
            && self.values().zip(other.values()).any(|(x, y)| x < y)
    }
}

impl PartialEq for Pareto<'static> {
    fn eq(&self, other: &Self) -> bool {
        self.values().zip(other.values()).all(|(s, o)| s.eq(o))
    }
}

static UNNAMED_OBJECTIVES: [&str; 10] = [
    "objective_0",
    "objective_1",
    "objective_2",
    "objective_3",
    "objective_4",
    "objective_5",
    "objective_6",
    "objective_7",
    "objective_8",
    "objective_9",
];

impl From<Vec<f64>> for Pareto<'static> {
    fn from(vec: Vec<f64>) -> Self {
        let mut map = BTreeMap::new();
        for (i, v) in vec.iter().enumerate() {
            map.insert(UNNAMED_OBJECTIVES[i], *v);
        }
        Pareto(map)
    }
}

impl Into<Vec<f64>> for Pareto<'static> {
    fn into(self) -> Vec<f64> {
        self.values().cloned().collect::<Vec<f64>>()
    }
}

// impl AsRef<[f64]> for Pareto<'static> {
//     fn as_ref(&self) -> &[f64] {
//         &self.0.values()
//     }
// }

impl Index<&str> for Pareto<'static> {
    type Output = f64;

    fn index(&self, i: &str) -> &Self::Output {
        &self
            .0
            .get(i)
            .unwrap_or_else(|| panic!("Invalid index for Pareto instance: {:?}", i))
    }
}

impl Index<usize> for Pareto<'static> {
    type Output = f64;

    fn index(&self, i: usize) -> &Self::Output {
        self.values()
            .nth(i)
            .expect("Invalid numeric index for Pareto")
    }
}

impl fmt::Debug for Pareto<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Pareto [")?;
        self.0
            .iter()
            .sorted_by_key(|p| p.0)
            .map(|(obj, score)| writeln!(f, "\t{} => {},", obj, score))
            .collect::<Result<Vec<()>, _>>()?;
        writeln!(f, "]")
    }
}

pub type Lexical<T> = Vec<T>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShuffleFit(#[serde(borrow)] BTreeMap<&'static str, f64>);

impl ShuffleFit {
    pub fn new() -> Self {
        Self(FitnessMap::new())
    }

    pub fn values(&self) -> impl Iterator<Item = &f64> {
        self.inner().iter().sorted_by_key(|p| p.0).map(|(_k, v)| v)
    }

    pub fn epoch_key(&self) -> &'static str {
        let epoch = crate::get_epoch_counter();
        let mut hasher = fnv::FnvHasher::default();
        epoch.hash(&mut hasher);
        let h = hasher.finish() as usize;
        let keys = self.0.keys().collect::<Vec<_>>();
        keys[h % keys.len()]
    }
}

impl HasScalar for ShuffleFit {
    fn scalar(&self) -> f64 {
        self.values().sum()
    }
}

impl MapFit for ShuffleFit {
    fn inner_mut(&mut self) -> &mut BTreeMap<&'static str, f64> {
        &mut self.0
    }

    fn inner(&self) -> &BTreeMap<&'static str, f64> {
        &self.0
    }

    fn from_map(map: BTreeMap<&'static str, f64>) -> Self {
        Self(map)
    }
}

impl PartialOrd for ShuffleFit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let k = self.epoch_key();
        self.0[k].partial_cmp(&other.0[k])
    }
}

impl Index<usize> for ShuffleFit {
    type Output = f64;

    fn index(&self, i: usize) -> &Self::Output {
        self.values()
            .nth(i)
            .expect("Invalid numeric index for Pareto")
    }
}

impl Index<&str> for ShuffleFit {
    type Output = f64;

    fn index(&self, i: &str) -> &Self::Output {
        &self
            .0
            .get(i)
            .unwrap_or_else(|| panic!("Invalid index for Pareto instance: {:?}", i))
    }
}

impl From<BTreeMap<&'static str, f64>> for ShuffleFit {
    fn from(map: BTreeMap<&'static str, f64>) -> Self {
        Self::from_map(map)
    }
}

impl FitnessScore for ShuffleFit {}

#[derive(Serialize, Deserialize)]
pub struct Weighted<'a> {
    weighting: String,
    #[serde(borrow)]
    pub scores: BTreeMap<&'a str, f64>,
    cached_scalar: Mutex<Option<f64>>,
}

impl PartialEq for Weighted<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.scores == other.scores && self.weighting == other.weighting
    }
}

impl std::ops::Add for Weighted<'static> {
    type Output = Weighted<'static>;

    fn add(self, rhs: Self) -> Self::Output {
        add_weighted(&self, &rhs)
        // let mut res = self.clone();
        // let mut keys = rhs.scores.keys().collect::<Vec<&&'static str>>();
        // keys.extend(self.scores.keys());
        // keys.sort();
        // keys.dedup();
        // for k in keys.into_iter() {
        //     *res.scores.entry(*k).or_insert(0.0) += rhs.scores.get(*k).cloned().unwrap_or(0.0);
        // }
        // res
    }
}

fn add_weighted(a: &Weighted<'static>, b: &Weighted<'static>) -> Weighted<'static> {
    let mut res = a.clone();
    let mut keys = a.scores.keys().collect::<Vec<_>>();
    keys.extend(b.scores.keys());
    keys.sort();
    keys.dedup();
    for k in keys.into_iter() {
        *res.scores.entry(*k).or_insert(0.0) += b.scores.get(*k).cloned().unwrap_or(0.0);
    }
    res
}

impl Clone for Weighted<'_> {
    fn clone(&self) -> Self {
        Self {
            cached_scalar: Mutex::new(None),
            weighting: self.weighting.clone(),
            scores: self.scores.clone(),
        }
    }
}

impl Weighted<'static> {
    pub fn new(weighting: &str) -> Self {
        Self {
            weighting: weighting.to_string(),
            scores: FitnessMap::new(),
            cached_scalar: Mutex::new(None),
        }
    }

    fn powf(&self, n: f64) -> Self {
        let mut res = self.clone();
        for v in res.scores.values_mut() {
            *v = v.powf(n);
        }
        res
    }

    pub fn values(&self) -> impl Iterator<Item = &f64> {
        self.scores.iter().sorted_by_key(|p| p.0).map(|(_k, v)| v)
    }

    pub fn scale_by(&mut self, factor: f64) {
        for (_, v) in self.scores.iter_mut() {
            *v = *v / factor
        }
    }

    pub fn insert(&mut self, key: &'static str, val: f64) {
        self.scores.insert(key, val);
    }

    pub fn get(&self, key: &'static str) -> Option<&f64> {
        self.scores.get(key)
    }

    pub fn insert_or_add(&mut self, key: &'static str, val: f64) {
        *self.scores.entry(key).or_insert(0.0) += val
    }

    pub fn scalar(&self) -> f64 {
        let mut cache = self.cached_scalar.lock().expect("poisoned");
        if let Some(res) = *cache {
            return res;
        } else {
            let res = self.scalar_with_expression(&self.weighting);
            *cache = Some(res);
            res
        }
    }

    pub fn scalar_with_expression(&self, expr: &str) -> f64 {
        if self.scores.is_empty() {
            return f64::MAX;
        }
        let mut ns = self.scores.clone();
        match fasteval::ez_eval(expr, &mut ns) {
            Err(e) => panic!(
                "Failed to evaluate expression {:?} with scores {:?}: {:?}",
                expr, self.scores, e
            ),
            Ok(res) => res,
        }
    }

    pub fn declare_failure(&mut self) {
        *self.cached_scalar.get_mut().unwrap() = Some(f64::MAX)
    }
}

pub fn average_weighted(ws: &[Weighted<'static>]) -> Weighted<'static> {
    let len = ws.len();
    let mut iter = ws.iter();
    let mut first = iter
        .next()
        .expect("weight vector must not be empty")
        .clone();
    for w in iter {
        first = add_weighted(&first, w);
    }
    first.scale_by(len as f64);
    first
}

pub fn stdev_weighted(ws: &[Weighted<'static>], mean: &Weighted<'static>) -> Weighted<'static> {
    let mut neg_mean = mean.clone();
    neg_mean.scale_by(-1.0);
    let mut res = Weighted::new(&neg_mean.weighting);
    for w in ws
        .iter()
        .map(|w| add_weighted(w, &neg_mean))
        .map(|w| w.powf(2.0))
    {
        res = add_weighted(&res, &w);
    }
    res.scale_by(ws.len() as f64 - 1.0);
    res.powf(0.5) // square root
}

impl HasScalar for Weighted<'static> {
    fn scalar(&self) -> f64 {
        Weighted::scalar(&self)
    }
}

impl PartialOrd for Weighted<'static> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.scalar().partial_cmp(&other.scalar())
    }
}

impl FitnessScore for Weighted<'static> {}

impl MapFit for Weighted<'static> {
    fn inner_mut(&mut self) -> &mut BTreeMap<&'static str, f64> {
        &mut self.scores
    }

    fn inner(&self) -> &BTreeMap<&'static str, f64> {
        &self.scores
    }

    fn from_map(_map: BTreeMap<&'static str, f64>) -> Self
    where
        Self: Sized,
    {
        unimplemented!("doesn't really make sense for Weighted")
    }
}

impl Index<&str> for Weighted<'static> {
    type Output = f64;

    fn index(&self, index: &str) -> &Self::Output {
        &self.scores[index]
    }
}

impl fmt::Debug for Weighted<'static> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Scores:")?;
        for (attr, score) in self.scores.iter().sorted_by_key(|p| p.0) {
            writeln!(f, "    {}: {}", attr, score)?;
        }
        writeln!(f, "Weighting expression: {}", self.weighting)?;
        writeln!(f, "Scalar: {}", self.scalar())
    }
}

#[cfg(test)]
mod test {
    use crate::pareto;

    use super::*;

    #[test]
    fn test_pareto_ordering() {
        let p1: Pareto<'static> = pareto! {"obj_a" => 0.1, "swankiness" => 2.0, "doom" => 3.1, };
        let p2: Pareto<'static> = pareto! {"obj_a" => 0.1, "swankiness" => 1.9, "doom" => 3.1, };
        let mut ps = vec![&p1, &p2];
        ps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        assert_eq!(ps[0], &p2);
    }

    #[test]
    fn test_add_weighted() {
        let mut w1 = Weighted::new("foo + 2 * bar");
        w1.insert("foo", 0.5);
        w1.insert("bar", 0.25);

        let mut w2 = Weighted::new("foo + 2 * bar");
        w2.insert("foo", 0.1);
        w2.insert("bar", 0.75);

        // 0.5 + 2 * 0.25 = 0.5 + 0.5 = 1.0
        assert_eq!(w1.scalar(), 1.0);
        // 0.1 + 2 * 0.75 = 0.1 + 1.50 = 1.6
        assert_eq!(w2.scalar(), 1.6);

        let w_sum = w1 + w2;
        let foo = w_sum.get("foo").cloned().unwrap();
        let bar = w_sum.get("bar").cloned().unwrap();
        assert_eq!(foo, 0.6);
        assert_eq!(bar, 1.0);
        assert_eq!(w_sum.scalar(), 2.6);
    }

    #[test]
    fn test_average_and_stddev_weighted() {
        let mut w1 = Weighted::new("foo + bar");
        w1.insert("foo", 1.0);
        w1.insert("bar", 2.0);
        let mut w2 = Weighted::new("foo + bar");
        w2.insert("foo", 2.0);
        w2.insert("bar", 1.0);
        let mut w3 = Weighted::new("foo + bar");
        w3.insert("foo", 4.0);
        w3.insert("bar", 0.5);
        let ws = &[w1, w2, w3];
        let w_mean = average_weighted(ws);
        println!("w_mean = {:?}", w_mean);
        let w_mean_foo = w_mean.get("foo").cloned().unwrap();
        let w_mean_bar = w_mean.get("bar").cloned().unwrap();
        assert_eq!(w_mean_foo, 2.3333333333333335); // 2.3333... repeating
        assert_eq!(w_mean_bar, 1.1666666666666667);

        let std_dev = stdev_weighted(ws, &w_mean);
        println!("std_dev = {:?}", std_dev);
        let s_foo = std_dev.get("foo").cloned().unwrap();
        let s_bar = std_dev.get("bar").cloned().unwrap();
        assert_eq!(s_foo, 1.5275252316519465);
        assert_eq!(s_bar, 0.7637626158259733);
    }
    // #[test]
    // fn test_find_minima() {
    //     fn random_pareto() -> Pareto<'static> {
    //         let mut par = Pareto::new();
    //         for i in 0..10 {
    //             par.insert(UNNAMED_OBJECTIVES[i], rand::random::<f64>());
    //         }
    //         par
    //     }
    //
    //     let sample = iter::repeat(())
    //         .take(100)
    //         .map(|()| random_pareto())
    //         .collect::<Vec<Pareto>>();
    //
    //     let mut minima = HashSet::new();
    //     for x in sample {}
    // }
}
