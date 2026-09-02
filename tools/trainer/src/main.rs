use bullet_lib::{
    game::inputs::Chess768,
    nn::optimiser::AdamW,
    trainer::{
        save::SavedFormat,
        schedule::{TrainingSchedule, TrainingSteps, lr, wdl},
        settings::LocalSettings,
    },
    value::{ValueTrainerBuilder, loader::DirectSequentialDataLoader},
};

/// Must match `HIDDEN` in engine/src/nnue.rs.
const HL: usize = 128;

const DATASET: &str = "../../../turbo9000-data/shuffled.bin";

/// 10_001_703 positions / 16_384 per batch, so one superbatch is one epoch.
const BATCH_SIZE: usize = 16_384;
const BATCHES_PER_SUPERBATCH: usize = 610;
const SUPERBATCHES: usize = 40;

/// Weight on the game result rather than the search score. The scores in
/// generation 0 are a depth 6 material readout, so the result carries most of
/// the signal.
const WDL_PROPORTION: f32 = 0.8;

fn main() {
    let initial_lr = 0.001;
    let final_lr = 0.001 * 0.3f32.powi(5);

    let mut trainer = ValueTrainerBuilder::default()
        .dual_perspective()
        .optimiser(AdamW)
        .inputs(Chess768)
        .save_format(&[
            SavedFormat::id("l0w").round().quantise::<i16>(255),
            SavedFormat::id("l0b").round().quantise::<i16>(255),
            SavedFormat::id("l1w").round().quantise::<i16>(64),
            SavedFormat::id("l1b").round().quantise::<i16>(255 * 64),
        ])
        .loss_fn(|output, target| output.sigmoid().squared_error(target))
        .build(|builder, stm_inputs, ntm_inputs| {
            let l0 = builder.new_affine("l0", 768, HL);
            let l1 = builder.new_affine("l1", 2 * HL, 1);

            let stm_hidden = l0.forward(stm_inputs).screlu();
            let ntm_hidden = l0.forward(ntm_inputs).screlu();
            l1.forward(stm_hidden.concat(ntm_hidden))
        });

    // A run that dies partway can be picked up from its last checkpoint:
    //
    //     RESUME_FROM=checkpoints/turbo9000-01-35 START_SUPERBATCH=36 cargo r -r --features cpu
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
        net_id: "turbo9000-01".to_string(),
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
