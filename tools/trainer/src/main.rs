use bullet_lib::{
    game::{
        inputs::{ChessBucketsMirrored, get_num_buckets},
        outputs::MaterialCount,
    },
    nn::{
        InitSettings, Shape,
        optimiser::{AdamW, AdamWParams},
    },
    trainer::{
        save::SavedFormat,
        schedule::{TrainingSchedule, TrainingSteps, lr, wdl},
        settings::LocalSettings,
    },
    value::{ValueTrainerBuilder, loader::DirectSequentialDataLoader},
};

/// Must match `HIDDEN` in engine/src/nnue.rs.
const HL: usize = 512;

/// Must match `OUTPUT_BUCKETS` in engine/src/nnue.rs.
const OUTPUT_BUCKETS: usize = 8;

/// Must match `KING_BUCKETS` in engine/src/nnue.rs. Mirrored onto the files a
/// to d, so index `rank * 4 + file`.
#[rustfmt::skip]
const KING_BUCKETS: [usize; 32] = [
    0, 0, 1, 1,
    0, 0, 1, 1,
    2, 2, 2, 2,
    2, 2, 2, 2,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
];

const INPUT_BUCKETS: usize = get_num_buckets(&KING_BUCKETS);

const DATASET: &str = "../../data/shuffled.data";

/// 30_001_300 positions / 16_384 per batch, so one superbatch is one epoch.
const BATCH_SIZE: usize = 16_384;
const BATCHES_PER_SUPERBATCH: usize = 1831;
const SUPERBATCHES: usize = 30;

/// Weight on the game result rather than the search score. Generation 1 scores
/// come out of a real search, so they carry much more than the generation 0
/// material readout did and the result is worth less.
const WDL_PROPORTION: f32 = 0.5;

fn main() {
    let initial_lr = 0.001;
    let final_lr = 0.001 * 0.3f32.powi(5);

    let mut trainer = ValueTrainerBuilder::default()
        .dual_perspective()
        .optimiser(AdamW)
        .inputs(ChessBucketsMirrored::new(KING_BUCKETS))
        .output_buckets(MaterialCount::<OUTPUT_BUCKETS>)
        .save_format(&[
            // The factoriser is a bucket agnostic copy of the input weights,
            // folded back in here so the engine only sees the buckets.
            SavedFormat::id("l0w")
                .transform(|store, weights| {
                    let factoriser = store.get("l0f").values.f32().repeat(INPUT_BUCKETS);
                    weights.into_iter().zip(factoriser).map(|(a, b)| a + b).collect()
                })
                .round()
                .quantise::<i16>(255),
            SavedFormat::id("l0b").round().quantise::<i16>(255),
            SavedFormat::id("l1w").round().quantise::<i16>(64).transpose(),
            SavedFormat::id("l1b").round().quantise::<i16>(255 * 64),
        ])
        .loss_fn(|output, target| output.sigmoid().squared_error(target))
        .build(|builder, stm_inputs, ntm_inputs, output_buckets| {
            let l0f = builder.new_weights("l0f", Shape::new(HL, 768), InitSettings::Zeroed);
            let mut l0 = builder.new_affine("l0", 768 * INPUT_BUCKETS, HL);
            l0.weights = l0.weights + l0f.repeat(INPUT_BUCKETS);

            let l1 = builder.new_affine("l1", 2 * HL, OUTPUT_BUCKETS);

            let stm_hidden = l0.forward(stm_inputs).screlu();
            let ntm_hidden = l0.forward(ntm_inputs).screlu();
            l1.forward(stm_hidden.concat(ntm_hidden)).select(output_buckets)
        });

    // The factoriser and the buckets each hold part of an input weight, so
    // both are clipped tighter to keep their sum inside the quantisation.
    let stricter_clipping = AdamWParams { max_weight: 0.99, min_weight: -0.99, ..Default::default() };
    trainer.optimiser.set_params_for_weight("l0w", stricter_clipping);
    trainer.optimiser.set_params_for_weight("l0f", stricter_clipping);

    // A run that dies partway can be picked up from its last checkpoint:
    //
    //     RESUME_FROM=checkpoints/turbo9000-02-25 START_SUPERBATCH=26 cargo r -r --features cuda
    //
    // `start_superbatch` also offsets the LR scheduler, so the cosine tail is
    // the same as it would have been in one uninterrupted run.
    let start_superbatch: usize =
        std::env::var("START_SUPERBATCH").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    if let Ok(path) = std::env::var("RESUME_FROM") {
        trainer.load_from_checkpoint(&path);
        println!("Resumed from {path}, starting at superbatch {start_superbatch}");
    }

    let schedule = TrainingSchedule {
        net_id: "turbo9000-02".to_string(),
        eval_scale: 400.0,
        steps: TrainingSteps {
            batch_size: BATCH_SIZE,
            batches_per_superbatch: BATCHES_PER_SUPERBATCH,
            start_superbatch,
            end_superbatch: SUPERBATCHES,
        },
        wdl_scheduler: wdl::ConstantWDL { value: WDL_PROPORTION },
        lr_scheduler: lr::CosineDecayLR { initial_lr, final_lr, final_superbatch: SUPERBATCHES },
        save_rate: 5,
    };

    let settings =
        LocalSettings { threads: 4, test_set: None, output_directory: "checkpoints", batch_queue_size: 32 };

    let dataloader = DirectSequentialDataLoader::new(&[DATASET]);

    trainer.run(&schedule, &settings, &dataloader);
}
