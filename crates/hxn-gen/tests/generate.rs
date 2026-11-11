use hxn_gen::{Receiver, Type, parse};

const SOURCE: &str = r#"
/// A device that plays sound.
#[hxn::interface]
pub struct AudioDevice {
    handle: u64,
}

#[hxn::interface]
pub struct Mixer;

struct NotExposed;

#[hxn::methods]
impl AudioDevice {
    pub fn new(path: &str, sample_rate: i32) -> Result<Self, OpenError> {
        todo!()
    }

    /// Queues samples.
    /// Returns how many were taken.
    pub fn write(&mut self, pcm: &[u8]) -> Result<i32, WriteError> {
        todo!()
    }

    pub fn is_open(&self) -> bool {
        true
    }

    #[hxn(blocking)]
    pub fn drain(&mut self) {}

    pub fn default_gain() -> f64 {
        1.0
    }

    pub fn name(&self) -> String {
        String::new()
    }

    pub fn route(&mut self, mixer: &mut Mixer, fallback: Option<&AudioDevice>) -> Option<Mixer> {
        None
    }

    pub fn duplicate(&self) -> Result<AudioDevice, OpenError> {
        todo!()
    }

    pub fn finish(self) -> i32 {
        0
    }

    pub fn play(&self, position: &AtomicI32, cancel: &AtomicFlag) {}

    pub fn level(&self) -> Arc<AtomicI32> {
        todo!()
    }

    pub fn muted(&self) -> Arc<AtomicFlag> {
        todo!()
    }

    fn internal(&self) {}
}

#[hxn::methods]
impl Mixer {
    pub fn attach(&self, device: &AudioDevice) {}
}

impl AudioDevice {
    pub fn not_exposed<T>(&self, value: T) {}
}
"#;

fn handle(interface: &str, nullable: bool) -> Type {
    Type::Handle {
        interface: interface.to_owned(),
        nullable,
    }
}

#[test]
fn reads_the_marked_items_and_nothing_else() {
    let library = parse("audio", SOURCE).unwrap();
    assert_eq!(library.name, "audio");
    let names: Vec<_> = library.interfaces.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["AudioDevice", "Mixer"]);
    let device = &library.interfaces[0];
    assert_eq!(device.docs, ["A device that plays sound."]);
    let names: Vec<_> = device.methods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "new",
            "write",
            "is_open",
            "drain",
            "default_gain",
            "name",
            "route",
            "duplicate",
            "finish",
            "play",
            "level",
            "muted"
        ]
    );

    let new = &device.methods[0];
    assert!(new.constructor && new.fallible);
    assert_eq!(new.receiver, Receiver::None);
    assert_eq!(new.returns, handle("AudioDevice", false));
    let params: Vec<_> = new
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.ty.clone()))
        .collect();
    assert_eq!(params, [("path", Type::Str), ("sample_rate", Type::I32)]);

    let write = &device.methods[1];
    assert_eq!(write.receiver, Receiver::Exclusive);
    assert_eq!(write.params[0].ty, Type::Bytes);
    assert_eq!((write.returns.clone(), write.fallible), (Type::I32, true));
    assert_eq!(
        write.docs,
        ["Queues samples.", "Returns how many were taken."]
    );

    assert_eq!(device.methods[2].receiver, Receiver::Shared);
    assert_eq!(device.methods[2].returns, Type::Bool);
    assert!(device.methods[3].blocking && !device.methods[2].blocking);
    assert_eq!(device.methods[3].returns, Type::Void);
    assert_eq!(device.methods[4].receiver, Receiver::None);
    assert!(!device.methods[4].constructor);
    assert_eq!(device.methods[5].returns, Type::Str);

    // Another interface borrowed, one that may be absent, and one handed back.
    let route = &device.methods[6];
    assert_eq!(route.params[0].ty, handle("Mixer", false));
    assert!(route.params[0].exclusive);
    assert_eq!(route.params[1].ty, handle("AudioDevice", true));
    assert!(!route.params[1].exclusive);
    assert_eq!(route.returns, handle("Mixer", true));

    let duplicate = &device.methods[7];
    assert_eq!(duplicate.returns, handle("AudioDevice", false));
    assert!(duplicate.fallible && !duplicate.constructor);
    assert_eq!(device.methods[8].receiver, Receiver::Owned);

    let attach = &library.interfaces[1].methods[0];
    assert_eq!(attach.params[0].ty, handle("AudioDevice", false));
}

fn refused(method: &str) -> (usize, String) {
    let source = format!(
        "#[hxn::interface]\npub struct Thing;\n\n#[hxn::methods]\nimpl Thing {{\n    {method}\n}}\n"
    );
    let error = parse("thing", &source).unwrap_err();
    (error.line, error.message)
}

#[test]
fn says_where_and_why_something_cannot_cross() {
    let cases = [
        ("pub fn each<T>(&self, value: T) {}", "generic"),
        ("pub async fn later(&self) {}", "async"),
        ("pub unsafe fn raw(&self) {}", "unsafe"),
        (
            "pub fn size(&self, value: usize) {}",
            "a parameter is one of",
        ),
        (
            "pub fn text(&self, value: String) {}",
            "a parameter is one of",
        ),
        (
            "pub fn other(&self, value: &Unmarked) {}",
            "a parameter is one of",
        ),
        ("pub fn eat(&self, value: Thing) {}", "not by value"),
        (
            "pub fn items(&self) -> Vec<u8> { todo!() }",
            "a result is one of",
        ),
        (
            "pub fn view(&self) -> &Thing { todo!() }",
            "a result cannot borrow",
        ),
        (
            "pub fn keep(&self, cell: Arc<AtomicI32>) {}",
            "a cell is lent for the call",
        ),
        (
            "pub fn cell(&self) -> AtomicI32 { todo!() }",
            "returned as `Arc<AtomicI32>`",
        ),
        (
            "pub fn new(&self) -> Self { todo!() }",
            "`new` is the constructor",
        ),
        ("pub fn new() -> i32 { 0 }", "`new` is the constructor"),
        ("pub fn close(&mut self) {}", "`close` is what releases"),
        ("pub fn take(&self, out: i32) {}", "a name the adapters use"),
        (
            "#[hxn(inline)]\n    pub fn noted(&self) {}",
            "unknown note `inline`",
        ),
        (
            "pub fn twice(&self) {}\n    pub fn twice(&mut self) {}",
            "exposed twice",
        ),
    ];
    for (method, expected) in cases {
        let (line, message) = refused(method);
        assert!(message.contains(expected), "{method}: {message}");
        assert!((6..=7).contains(&line), "{method}: line {line}");
    }
}

#[test]
fn refuses_what_is_marked_wrongly() {
    let unmarked = "pub struct Thing;\n\n#[hxn::methods]\nimpl Thing {}\n";
    let error = parse("thing", unmarked).unwrap_err();
    assert_eq!(error.line, 4);
    assert!(error.message.contains("not marked `#[hxn::interface]`"));

    let private = "#[hxn::interface]\nstruct Thing;\n";
    assert!(
        parse("thing", private)
            .unwrap_err()
            .message
            .contains("`pub`")
    );

    let broken = "pub struct {";
    assert!(parse("thing", broken).unwrap_err().line > 0);
}

#[test]
fn names_follow_each_language() {
    let library = parse("audio", SOURCE).unwrap();
    let adapter = hxn_gen::hl_adapter(&library, "audio_core");
    assert!(
        adapter.contains("pub unsafe extern \"C\" fn audio_audio_device_default_gain() -> f64")
    );
    assert!(adapter.contains("define_prim!(hlp_audio_device_write, audio_audio_device_write, \"PXaudio_audio_device_Bi_i\");"));
    assert!(adapter.contains("blocking(|| this.drain())"));
    assert!(adapter.contains("use audio_core::{AudioDevice, Mixer};"));

    let header = hxn_gen::c_header(&library);
    assert!(header.contains("hxn_status audio_audio_device_new(hxn_slice path, int32_t sample_rate, hxn_handle **out, hxn_buf *err);"));
    assert!(header.contains("void audio_release(hxn_handle *self);"));

    let glue = hxn_gen::cpp_glue(&library);
    assert!(glue.contains("namespace audio_glue {"));
    assert!(glue.contains("hxn::Blocking zone;"));

    let haxe = &hxn_gen::haxe_hl(&library)[0];
    assert_eq!(haxe.path, "AudioDevice.hx");
    assert!(
        haxe.text
            .contains("public function new(path:String, sampleRate:Int)")
    );
    assert!(
        haxe.text
            .contains("public static function defaultGain():Float")
    );
    assert!(haxe.text.contains("public function isOpen():Bool"));
    assert!(haxe.text.contains("public function close():Void"));
    assert!(
        hxn_gen::haxe_cpp(&library)[0]
            .text
            .contains("@:headerInclude(\"audio_glue.h\")")
    );
}

#[test]
fn objects_are_borrowed_going_in_and_owned_coming_out() {
    let library = parse("audio", SOURCE).unwrap();

    let c = hxn_gen::c_adapter(&library, "audio_core");
    assert!(c.contains("let mut mixer = Handle::<Mixer>::from_raw(mixer)?.borrow_mut()?;"));
    assert!(c.contains("let fallback = if fallback.is_null() { None } else { Some(Handle::<AudioDevice>::from_raw(fallback)?.borrow()?) };"));
    assert!(c.contains(
        "this.route(&mut mixer, fallback.as_deref()).map_or(null_mut(), Handle::into_raw)"
    ));
    assert!(c.contains("Ok(Handle::into_raw(this.duplicate()?))"));
    assert!(c.contains(
        "let this = Handle::<AudioDevice>::from_raw(this)?.take()?.ok_or(Fault::Closed)?;"
    ));

    let hl = hxn_gen::hl_adapter(&library, "audio_core");
    assert!(
        hl.contains("\"PXaudio_audio_device_Xaudio_mixer_Xaudio_audio_device__Xaudio_mixer_\"")
    );
    assert!(hl.contains("let this = handle::<AudioDevice>(this)?.take()?.ok_or(Fault::Closed)?;"));

    let header = hxn_gen::c_header(&library);
    assert!(header.contains("hxn_status audio_audio_device_route(hxn_handle *self, hxn_handle *mixer, hxn_handle *fallback, hxn_handle **out, hxn_buf *err);"));

    let device = &hxn_gen::haxe_hl(&library)[0].text;
    assert!(
        device
            .contains("public function route(mixer:Mixer, fallback:Null<AudioDevice>):Null<Mixer>")
    );
    assert!(device.contains("mixer:Mixer.MixerRaw"));
    assert!(device.contains("mixer == null ? null : mixer.raw"));
    assert!(device.contains("return made == null ? null : Mixer.adopt(made);"));
    assert!(device.contains("return AudioDevice.adopt(Native.audio_device_duplicate(raw));"));
    assert!(device.contains("public function finish():Int"));

    let cpp = &hxn_gen::haxe_cpp(&library)[0].text;
    assert!(cpp.contains("var made = Mixer.adopt();"));
    assert!(cpp.contains("made.own();"));

    let manifest = hxn_gen::manifest(&library);
    assert!(
        manifest
            .contains("{ \"name\": \"mixer\", \"type\": \"Mixer\", \"borrow\": \"exclusive\" }")
    );
    assert!(manifest.contains("\"returns\": \"Mixer?\""));
    assert!(manifest.contains("\"kind\": \"consuming\""));
}

#[test]
fn cells_are_lent_going_in_and_kept_by_the_library_coming_out() {
    let library = parse("audio", SOURCE).unwrap();
    let device = &library.interfaces[0];
    let play = device.methods.iter().find(|m| m.name == "play").unwrap();
    assert_eq!(play.params[0].ty, Type::AtomicInt);
    assert_eq!(play.params[1].ty, Type::AtomicBool);
    assert!(library.returns_cells());

    let c = hxn_gen::c_adapter(&library, "audio_core");
    assert!(c.contains("position: *mut i32, cancel: *mut i32"));
    assert!(c.contains("AtomicFlag::from_ptr(cancel)"));
    assert!(c.contains("Ok(Handle::into_raw(AtomicFlag::into_int(this.muted())))"));
    assert!(c.contains("pub unsafe extern \"C\" fn audio_atomic_cell(this: RawHandle, out: *mut *mut i32, err: *mut Buf) -> Status"));

    let hl = hxn_gen::hl_adapter(&library, "audio_core");
    assert!(hl.contains("\"PXaudio_audio_device_RiRi_v\""));
    assert!(
        hl.contains("define_prim!(hlp_atomic_cell, audio_atomic_cell, \"PXaudio_atomic__Ri\");")
    );

    let header = hxn_gen::c_header(&library);
    assert!(header.contains("hxn_status audio_audio_device_play(hxn_handle *self, int32_t *position, int32_t *cancel, hxn_buf *err);"));
    assert!(
        header.contains(
            "hxn_status audio_atomic_cell(hxn_handle *self, int32_t **out, hxn_buf *err);"
        )
    );

    let haxe = &hxn_gen::haxe_hl(&library)[0].text;
    assert!(
        haxe.contains("public function play(position:hxn.AtomicInt, cancel:hxn.AtomicBool):Void")
    );
    assert!(haxe.contains("public function muted():hxn.AtomicBool"));
    assert!(haxe.contains("return hxn.AtomicInt.over(Native.atomic_cell(owner), () -> owner);"));

    let cpp = hxn_gen::haxe_cpp(&library);
    assert_eq!(cpp.last().unwrap().path, "HxnAudioCell.hx");
    assert!(cpp[0].text.contains("var owner = HxnAudioCell.adopt();"));

    let manifest = hxn_gen::manifest(&library);
    assert!(manifest.contains("{ \"name\": \"cancel\", \"type\": \"atomic_flag\" }"));
    assert!(manifest.contains("\"returns\": \"atomic_i32\""));
}

// The fixture's manifest is committed, as a library's would be, so a change
// to what it exposes shows up in review. To update it after changing
// tests/counter/core, run hxn-gen and copy its counter.hxn.json over.
#[test]
fn the_counter_manifest_is_current() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/counter");
    let source = std::fs::read_to_string(format!("{root}/core/src/lib.rs")).unwrap();
    let committed = std::fs::read_to_string(format!("{root}/counter.hxn.json")).unwrap();
    let library = parse("counter", &source).unwrap();
    assert_eq!(hxn_gen::manifest(&library), committed);
    assert_eq!(
        hxn_gen::manifest(&library),
        hxn_gen::manifest(&library.clone())
    );
}
