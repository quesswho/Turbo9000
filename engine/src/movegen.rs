/// Stages of staged move generation, in the order of the search.
/// Each stage is entered at most once, and only the `Generate*` stages
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    TtMove,

    GenerateNoisy,
    /// Captures and promotions that do not lose material by static exchange.
    /// The losers are set aside for [`Stage::BadNoisy`].
    GoodNoisy,

    Killer1,
    Killer2,
    /// The quiet move that refuted the opponent's previous move.
    CounterMove,

    GenerateQuiet,
    Quiet,

    /// Captures static exchange evaluation says lose material, tried last
    /// because they are usually bad but occasionally the only way through.
    BadNoisy,

    /// In check the evasion generator replaces the noisy and quiet split
    /// entirely, since the legal moves are too few for staging to pay off.
    GenerateEvasion,
    Evasion,

    Done,
}
