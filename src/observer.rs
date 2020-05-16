use async_trait::async_trait;
use std::sync::Arc;

// A Logger needs to asynchronously gather and periodically
// record information on the evolutionary process.

#[async_trait]
pub trait Observe {
    type Observable;

    /// The observe method should take a clone of the observable
    /// and store in something like a sliding observation window.
    fn observe(&self, ob: Self::Observable);

    /// The report method should generate quantitative observations on
    /// the sliding window, and send those observations out to a logger
    /// (which will presumably record the data some serial format -- a
    /// tsv file, for instance, that can then be consumed by gnuplot.
    /// Or perhaps something more structured, like json.
    fn report(&self);

}
