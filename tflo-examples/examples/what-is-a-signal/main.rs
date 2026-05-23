use tflo_core::prelude::*;
use tflo_ops::events::ThresholdCrossEventMode;
use tflo_ops::prelude::*;

// An RF spectrum-monitoring record: a detection of an emitter on the band.
#[derive(Clone, Debug)]
struct Detection {
    ts: i64,
    snr_db: f64,
    /// Carrier frequency — domain context, not used by this example's graph.
    #[allow(dead_code)]
    freq_mhz: f64,
}

impl Detection {
    const fn new(ts: i64, snr_db: f64, freq_mhz: f64) -> Self {
        Self {
            ts,
            snr_db,
            freq_mhz,
        }
    }
}

// A domain-specific signal type: how the spectrum monitor should react.
#[allow(dead_code)] // Drop / Ignore illustrate the enum; only Track is emitted here.
#[derive(Clone, Debug, PartialEq)]
enum EmitterAction {
    Track,
    Drop,
    Ignore,
}

fn sample_detections() -> Vec<Detection> {
    vec![
        Detection::new(1000, 8.0, 2412.0),
        Detection::new(2000, 9.0, 2412.5),
        Detection::new(3000, 7.0, 2411.8),
        Detection::new(4000, 12.0, 2413.0),
        Detection::new(5000, 14.0, 2413.2),
        Detection::new(6000, 17.0, 2414.0),
        Detection::new(7000, 15.0, 2413.6),
        Detection::new(8000, 19.0, 2414.4),
        Detection::new(9000, 22.0, 2415.0),
        Detection::new(10000, 20.0, 2414.7),
    ]
}

fn main() {
    let detections = sample_detections();

    // ---- Signal type demonstration ----
    println!("=== Signal<TMode, TPayload> ===");
    println!();
    println!("Signal has two parts:");
    println!("  mode:    what kind of event (Rising, Falling, Entered, etc.)");
    println!("  payload: data attached to the event");
    println!();

    // Simple signal (no payload)
    println!("--- Simple (no payload) ---");
    let cross_signal = Signal::simple(ThresholdCrossEventMode::Rising);
    println!("ThresholdCross signal: mode={:?}", cross_signal.mode);
    println!(
        "  is_active={}",
        cross_signal.mode != ThresholdCrossEventMode::None
    );

    // Signal with payload
    println!("\n--- Signal with payload ---");
    let emitter = Signal::new(EmitterAction::Track, 14.50);
    println!(
        "Emitter signal: mode={:?}, payload={}",
        emitter.mode, emitter.payload
    );

    // Transform payload
    let doubled = Signal::new(ThresholdCrossEventMode::Rising, 10.0).map_payload(|x| x * 2.0);
    println!("Mapped payload: {}", doubled.payload);

    // ---- ThresholdCrossEventMode from cross detection ----
    println!("\n--- Cross detection signals ---");
    let signals: Vec<(f64, ThresholdCrossEventMode, ThresholdCrossEventMode)> = detections
        .clone()
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let snr_db = t.prop(|x| x.snr_db);
            let sma = snr_db.sma(3usize);
            let above = snr_db.cross_above(&sma);
            let below = snr_db.cross_under(&sma);
            (sma, above, below)
        })
        .collect();

    for (ts, (sma, above, below)) in detections.iter().map(|d| d.ts).zip(&signals) {
        let desc = match (above, below) {
            (ThresholdCrossEventMode::Rising, _) => "SNR CROSSED ABOVE BASELINE".to_string(),
            (_, ThresholdCrossEventMode::Falling) => "SNR CROSSED BELOW BASELINE".to_string(),
            _ => "no cross".to_string(),
        };
        if *above != ThresholdCrossEventMode::None || *below != ThresholdCrossEventMode::None {
            println!("  ts={ts:>6} baseline={sma:.4} dB → {desc}");
        }
    }

    // ---- Custom signal pipeline ----
    println!("\n--- Custom signal pipeline ---");
    let track_signals: Vec<ThresholdCrossEventMode> = detections
        .into_iter()
        .tflo(|t| {
            t.timestamp(|x| x.ts);
            let snr_db = t.prop(|x| x.snr_db);
            let sma = snr_db.sma(3usize);
            let rsi = snr_db.rsi(14usize);
            // Signal when SNR crosses its rolling baseline
            let cross = snr_db.cross_above(&sma);
            // And SNR momentum crosses above 30
            let _momentum_signal = rsi.cross_above(&t.constant(30.0));
            // Just return the SNR cross for now
            cross
        })
        .collect();

    println!("Track signal events (SNR crossing baseline while momentum is climbing):");
    for (i, sig) in track_signals.iter().enumerate() {
        if *sig != ThresholdCrossEventMode::None {
            println!("  idx={i}: {sig:?}");
        }
    }
}
