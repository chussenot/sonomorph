//! The mixers' contract: nothing on the audio callback's path touches the
//! heap. A busy stretch of play (music starting, crossfading and waiting on
//! a stinger, more effects than there are voices, a cut, the sound modes)
//! makes not one allocator call, in either engine.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use sonomorph::advanced;
use sonomorph::chip8::{self, mulaw, Command};

/// Counts allocator calls made on a thread while it is armed.
struct Counting;

static CALLS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

fn note() {
    if ARMED.with(Cell::get) {
        CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note();
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Allocator calls made by `f` on this thread.
fn calls_in(f: impl FnOnce()) -> usize {
    let before = CALLS.load(Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    CALLS.load(Ordering::Relaxed) - before
}

fn tone(secs: f32, hz: f32) -> &'static [u8] {
    let rate = chip8::MixerConfig::default().sample_rate;
    let bytes: Vec<u8> = (0..(secs * rate) as usize)
        .map(|i| {
            let s = (i as f32 / rate * hz * std::f32::consts::TAU).sin() * 0.8;
            mulaw::encode((s * 32_767.0) as i16)
        })
        .collect();
    Box::leak(bytes.into_boxed_slice())
}

/// 12,000 stereo frames of QOA at 48 kHz.
const REF_QOA: &[u8] = include_bytes!("../src/advanced/testdata/ref.qoa");

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cue {
    Effect,
    Fanfare,
    TrackA,
    TrackB,
}

/// Made before counting starts: in a game they are `include_bytes!`.
static CLIPS: std::sync::OnceLock<[&'static [u8]; 4]> = std::sync::OnceLock::new();

fn clips() -> [&'static [u8]; 4] {
    *CLIPS.get_or_init(|| {
        [
            tone(0.3, 880.0),
            tone(1.0, 440.0),
            tone(2.0, 220.0),
            tone(2.0, 330.0),
        ]
    })
}

impl chip8::Sound for Cue {
    fn pcm(self) -> &'static [u8] {
        clips()[self as usize]
    }
    fn holds_music(self) -> bool {
        self == Cue::Fanfare
    }
}

impl advanced::Sound for Cue {
    fn clip(self) -> advanced::Clip {
        match self {
            Cue::Effect => advanced::Clip::MuLaw(clips()[0]),
            Cue::Fanfare | Cue::TrackA => advanced::Clip::Qoa(REF_QOA),
            Cue::TrackB => advanced::Clip::MuLaw(clips()[3]),
        }
    }
    fn holds_music(self) -> bool {
        self == Cue::Fanfare
    }
}

/// The busy stretch, against any mixer.
fn busy(mut apply: impl FnMut(Command<Cue>), mut render: impl FnMut()) {
    apply(Command::Music(Some(Cue::TrackA)));
    for block in 0..600 {
        match block {
            40 => apply(Command::Music(Some(Cue::TrackB))),
            60 => apply(Command::Music(Some(Cue::TrackA))),
            100..=160 => apply(Command::Play {
                sound: Cue::Effect,
                gain: 0.6,
                pan: -0.3,
                pitch: 1.5,
                duck: 0.0,
            }),
            200 => apply(Command::Play {
                sound: Cue::Fanfare,
                gain: 0.7,
                pan: 0.0,
                pitch: 1.0,
                duck: 2.7,
            }),
            210 => apply(Command::Music(Some(Cue::TrackB))),
            260 => apply(Command::CutHeld),
            300 => apply(Command::MusicLevel(0.3)),
            340 => apply(Command::Enable {
                effects: false,
                music: true,
            }),
            380 => apply(Command::Enable {
                effects: true,
                music: true,
            }),
            420 => apply(Command::Music(None)),
            _ => {}
        }
        render();
    }
}

#[test]
fn the_chip8_callback_path_never_touches_the_heap() {
    clips();
    let mut mixer = chip8::Mixer::<Cue>::new(44_100);
    let mut out = vec![0.0f32; 2 * 512];
    let mixer = std::cell::RefCell::new(&mut mixer);
    let calls = calls_in(|| {
        busy(
            |c| mixer.borrow_mut().apply(c),
            || mixer.borrow_mut().render(&mut out, 2),
        )
    });
    assert_eq!(calls, 0, "allocator calls on the callback's path");
}

#[test]
fn the_advanced_callback_path_never_touches_the_heap() {
    clips();
    let mut mixer = advanced::Mixer::<Cue>::new(44_100);
    let mut out = vec![0.0f32; 2 * 512];
    let mixer = std::cell::RefCell::new(&mut mixer);
    let calls = calls_in(|| {
        busy(
            |c| mixer.borrow_mut().apply(c),
            || mixer.borrow_mut().render(&mut out, 2),
        )
    });
    assert_eq!(calls, 0, "allocator calls on the callback's path");
}

#[test]
fn the_counter_counts() {
    let calls = calls_in(|| drop(std::hint::black_box(vec![1u8; 64])));
    assert!(calls >= 2, "an allocation and a free: {calls}");
}
