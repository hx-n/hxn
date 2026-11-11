use hxn_gen::{Receiver, Type, parse};

const SOURCE: &str = r#"
/// A device that plays sound.
#[hxn::interface]
pub struct AudioDevice {
    handle: u64,
}

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

    fn internal(&self) {}
}

impl AudioDevice {
    pub fn not_exposed<T>(&self, value: T) {}
}
"#;

#[test]
fn reads_the_marked_items_and_nothing_else() {
    let library = parse("audio", SOURCE).unwrap();
    assert_eq!(library.name, "audio");
    assert_eq!(library.interfaces.len(), 1);
    let device = &library.interfaces[0];
    assert_eq!(device.name, "AudioDevice");
    assert_eq!(device.docs, ["A device that plays sound."]);
    let names: Vec<_> = device.methods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        ["new", "write", "is_open", "drain", "default_gain", "name"]
    );

    let new = &device.methods[0];
    assert!(new.is_constructor() && new.fallible);
    assert_eq!(new.receiver, Receiver::None);
    let params: Vec<_> = new.params.iter().map(|p| (p.name.as_str(), p.ty)).collect();
    assert_eq!(params, [("path", Type::Str), ("sample_rate", Type::I32)]);

    let write = &device.methods[1];
    assert_eq!(write.receiver, Receiver::Exclusive);
    assert_eq!(write.params[0].ty, Type::Bytes);
    assert_eq!((write.returns, write.fallible), (Type::I32, true));
    assert_eq!(
        write.docs,
        ["Queues samples.", "Returns how many were taken."]
    );

    assert_eq!(device.methods[2].receiver, Receiver::Shared);
    assert_eq!(device.methods[2].returns, Type::Bool);
    assert!(device.methods[3].blocking && !device.methods[2].blocking);
    assert_eq!(device.methods[3].returns, Type::Void);
    assert_eq!(device.methods[4].receiver, Receiver::None);
    assert!(!device.methods[4].is_constructor());
    assert_eq!(device.methods[5].returns, Type::Str);
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
        ("pub fn consume(self) {}", "`&self` or `&mut self`"),
        (
            "pub fn size(&self, value: usize) {}",
            "a parameter is one of",
        ),
        (
            "pub fn text(&self, value: String) {}",
            "a parameter is one of",
        ),
        (
            "pub fn items(&self) -> Vec<u8> { todo!() }",
            "a result is one of",
        ),
        ("pub fn copy(&self) -> Self { todo!() }", "only `new`"),
        ("pub fn make() -> Self { todo!() }", "only `new`"),
        ("pub fn close(&mut self) {}", "`close` is what releases"),
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
    assert!(adapter.contains("use audio_core::AudioDevice;"));

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
