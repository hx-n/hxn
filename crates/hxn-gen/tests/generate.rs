use hxn_gen::{File, Library, Number, Receiver, Type, parse};

// The hxcpp classes less the extern class they call the glue through, which
// HashLink has no file for: each of its classes declares its own natives.
fn cpp_classes(library: &Library) -> Vec<File> {
    let mut files = hxn_gen::haxe_cpp(library);
    let native = files.pop().unwrap();
    assert!(native.text.contains("extern class Hxn"), "{}", native.path);
    files
}

const SOURCE: &str = r#"
/// A device that plays sound.
#[hxn::interface]
pub struct AudioDevice {
    handle: u64,
}

#[hxn::interface(weighed)]
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
            "pub fn size(&self, value: u128) {}",
            "wider than 64 bits does not cross",
        ),
        (
            "pub fn stamp(&self) -> Result<i128, String> { todo!() }",
            "wider than 64 bits does not cross",
        ),
        ("pub fn later(&self) -> Future<u16> { todo!() }", "carries"),
        (
            "pub fn maybe(&self) -> Option<u8> { todo!() }",
            "a result is one of",
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
            "pub fn items(&self) -> Vec<i32> { todo!() }",
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

    let noted = "#[hxn::interface(heavy)]\npub struct Thing;\n";
    let error = parse("thing", noted).unwrap_err();
    assert_eq!(error.line, 1);
    assert!(error.message.contains("unknown note `heavy`"));

    for taken in ["Atomic", "Memory"] {
        let source = format!("#[hxn::interface]\npub struct {taken};\n");
        let error = parse("thing", &source).unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("a name the adapters use"));
    }
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

    // hxcpp calls the glue through one extern class, which also tells the
    // build what to include and link.
    let cpp = hxn_gen::haxe_cpp(&library);
    let native = &cpp.last().unwrap();
    assert_eq!(native.path, "HxnAudioNative.hx");
    assert!(native.text.contains("@:include(\"audio_glue.h\")"));
    assert!(native.text.contains("<lib name=\"${AUDIO_LIB}\"/>"));
    assert!(
        native.text.contains(
            "@:native(\"audio_glue::release\") static function release(self:Handle):Void;"
        )
    );
    for file in &cpp {
        assert!(!file.text.contains("__cpp__"), "{}", file.path);
        for (at, _) in file.text.match_indices("HxnAudioNative.") {
            let called = file.text[at + "HxnAudioNative.".len()..]
                .split('(')
                .next()
                .unwrap();
            assert!(
                native.text.contains(&format!("static function {called}(")),
                "{called} in {}",
                file.path
            );
        }
    }
    let glue = hxn_gen::cpp_glue(&library);
    for (at, _) in native.text.match_indices("@:native(\"audio_glue::") {
        let declared = native.text[at + "@:native(\"audio_glue::".len()..]
            .split('"')
            .next()
            .unwrap();
        assert!(
            glue.contains(&format!(" {declared}(")) || glue.contains(&format!("*{declared}(")),
            "{declared}"
        );
    }
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
    assert!(cpp.iter().any(|file| file.path == "HxnAudioCell.hx"));
    assert!(cpp[0].text.contains("var owner = HxnAudioCell.adopt();"));

    let manifest = hxn_gen::manifest(&library);
    assert!(manifest.contains("{ \"name\": \"cancel\", \"type\": \"atomic_flag\" }"));
    assert!(manifest.contains("\"returns\": \"atomic_i32\""));
}

#[test]
fn what_a_library_holds_is_read_and_a_weighed_object_checks_it() {
    let library = parse("audio", SOURCE).unwrap();
    assert!(!library.interfaces[0].weighed && library.interfaces[1].weighed);

    // Every library can be asked, whatever it marked.
    let c = hxn_gen::c_adapter(&library, "audio_core");
    assert!(c.contains("pub extern \"C\" fn audio_memory_held() -> f64"));
    assert!(c.contains("pub extern \"C\" fn audio_memory_wanted() -> i32"));
    let hl = hxn_gen::hl_adapter(&library, "audio_core");
    assert!(hl.contains("define_prim!(hlp_memory_held, audio_memory_held, \"P_d\");"));
    assert!(hl.contains("define_prim!(hlp_memory_wanted, audio_memory_wanted, \"P_b\");"));
    let header = hxn_gen::c_header(&library);
    assert!(header.contains("double audio_memory_held(void);"));
    assert!(header.contains("int32_t audio_memory_wanted(void);"));

    for files in [hxn_gen::haxe_hl(&library), cpp_classes(&library)] {
        let memory = files.last().unwrap();
        assert_eq!(memory.path, "HxnAudioMemory.hx");
        assert!(
            memory
                .text
                .contains("public static final meter = new hxn.Gc.Meter(")
        );
        // Each class holds the meter; only a weighed one checks it.
        let (device, mixer) = (&files[0].text, &files[1].text);
        assert!(device.contains("static final memory = HxnAudioMemory.meter;"));
        assert!(!device.contains("memory.check();"));
        assert!(mixer.contains("static final memory = HxnAudioMemory.meter;"));
        assert!(mixer.contains("memory.check();"));
    }

    // Where a value the library returned gets its owner, on each runtime.
    let mixer = &hxn_gen::haxe_hl(&library)[1].text;
    assert!(mixer.contains("made.raw = raw;\n\t\tmemory.check();\n\t\treturn made;"));
    let mixer = &hxn_gen::haxe_cpp(&library)[1].text;
    assert!(mixer.contains(
        "cpp.vm.Gc.setFinalizer(this, cpp.Function.fromStaticFunction(finalize));\n\t\tmemory.check();"
    ));

    let manifest = hxn_gen::manifest(&library);
    assert_eq!(manifest.matches("\"weighed\": true").count(), 1);
}

const SHARED: &str = r#"
#[hxn::interface(shared, weighed)]
pub struct Bus;

#[hxn::interface]
pub struct Tap {
    bus: Arc<Bus>,
}

#[hxn::methods]
impl Bus {
    pub fn new() -> Self {
        Bus
    }

    pub fn open() -> Arc<Bus> {
        todo!()
    }

    pub fn find(&self, name: &str) -> Option<Arc<Bus>> {
        None
    }
}

#[hxn::methods]
impl Tap {
    pub fn new(bus: Arc<Bus>) -> Self {
        todo!()
    }

    pub fn reads(&self, bus: &Bus) -> bool {
        false
    }

    pub fn rewire(&mut self, bus: Option<Arc<Bus>>, old: Option<&Bus>) -> Option<Bus> {
        None
    }
}
"#;

#[test]
fn a_shared_value_is_held_through_an_arc() {
    let library = parse("audio", SHARED).unwrap();
    let (bus, tap) = (&library.interfaces[0], &library.interfaces[1]);
    assert!(bus.shared && bus.weighed && !tap.shared);
    assert!(library.is_shared("Bus") && !library.is_shared("Tap"));
    assert!(!bus.methods[0].returns_arc && bus.methods[1].returns_arc);
    assert_eq!(bus.methods[2].returns, handle("Bus", true));
    assert!(tap.methods[0].params[0].kept && !tap.methods[1].params[0].kept);

    // A value is put in an `Arc` by the adapter unless it came in one, and a
    // parameter is the value or a share of it, as the function asked.
    let c = hxn_gen::c_adapter(&library, "audio_core");
    assert!(c.contains("Ok(Handle::into_raw(Arc::new(Bus::new())))"));
    assert!(c.contains("Ok(Handle::into_raw(Bus::open()))"));
    assert!(c.contains("let this = Handle::<Arc<Bus>>::from_raw(this)?.borrow()?;"));
    assert!(c.contains("Ok(this.find(name.as_str()?).map_or(null_mut(), Handle::into_raw))"));
    assert!(c.contains("Tap::new(Arc::clone(&bus))"));
    assert!(c.contains("Ok(i32::from(this.reads(&bus)))"));
    assert!(c.contains(
        "this.rewire(bus.as_deref().cloned(), old.as_deref().map(Arc::as_ref)).map(Arc::new).map_or(null_mut(), Handle::into_raw)"
    ));
    assert!(c.contains("Handle::<Arc<Bus>>::from_raw(this)?.take()?;"));
    assert!(c.contains("Ok(i32::from(Arc::ptr_eq(&this, &other)))"));
    assert!(c.contains("Ok(Handle::into_raw(Arc::downgrade(&this)))"));
    assert!(c.contains("let this = Handle::<Weak<Bus>>::from_raw(this)?.borrow()?;"));
    assert!(c.contains("pub unsafe extern \"C\" fn audio_bus_weak_close("));

    let hl = hxn_gen::hl_adapter(&library, "audio_core");
    assert!(hl.contains("Ok(manage(Arc::new(Bus::new())))"));
    assert!(
        hl.contains("define_prim!(hlp_bus_same, audio_bus_same, \"PXaudio_bus_Xaudio_bus__b\");")
    );
    assert!(hl.contains(
        "define_prim!(hlp_bus_weak, audio_bus_weak, \"PXaudio_bus__Xaudio_bus_weak_\");"
    ));
    assert!(hl.contains(
        "define_prim!(hlp_bus_weak_upgrade, audio_bus_weak_upgrade, \"PXaudio_bus_weak__Xaudio_bus_\");"
    ));
    assert!(hl.contains(
        "define_prim!(hlp_bus_weak_close, audio_bus_weak_close, \"PXaudio_bus_weak__v\");"
    ));

    let header = hxn_gen::c_header(&library);
    assert!(header.contains(
        "hxn_status audio_bus_same(hxn_handle *self, hxn_handle *other, int32_t *out, hxn_buf *err);"
    ));
    assert!(header.contains(
        "hxn_status audio_bus_weak_upgrade(hxn_handle *self, hxn_handle **out, hxn_buf *err);"
    ));
    let glue = hxn_gen::cpp_glue(&library);
    assert!(glue.contains("inline bool bus_same(void *self, void *other)"));
    assert!(glue.contains("inline void *bus_weak_upgrade(void *self)"));

    for files in [hxn_gen::haxe_hl(&library), cpp_classes(&library)] {
        let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            ["Bus.hx", "Tap.hx", "BusWeak.hx", "HxnAudioMemory.hx"]
        );
        let (bus, tap, weak) = (&files[0].text, &files[1].text, &files[2].text);
        assert!(bus.contains("public function same(other:Null<Bus>):Bool"));
        assert!(bus.contains("public function weak():BusWeak"));
        assert!(bus.contains("Gives up this object's share"));
        assert!(!tap.contains("function same(") && !tap.contains("function weak("));
        assert!(weak.contains("public function upgrade():Null<Bus>"));
        assert!(weak.contains("public function close():Void"));
    }

    let manifest = hxn_gen::manifest(&library);
    assert_eq!(manifest.matches("\"shared\": true").count(), 1);
}

#[test]
fn a_shared_value_is_never_borrowed_exclusively() {
    let cases = [
        (
            "pub fn set(&mut self) {}",
            "reached from more than one place",
        ),
        ("pub fn end(self) {}", "reached from more than one place"),
        (
            "pub fn other(&self, thing: &mut Thing) {}",
            "reached from more than one place",
        ),
        ("pub fn same(&self) {}", "has a `same` of hxn's"),
        ("pub fn weak(&self) {}", "has a `weak` of hxn's"),
        (
            "pub fn take(&self, thing: &Arc<Thing>) {}",
            "or `Arc<T>` to keep a share",
        ),
        (
            "pub fn keep(&self, plain: Arc<Plain>) {}",
            "needs `Plain` marked `#[hxn::interface(shared)]`",
        ),
        (
            "pub fn give(&self) -> Arc<Plain> { todo!() }",
            "needs `Plain` marked `#[hxn::interface(shared)]`",
        ),
        (
            "pub fn boxed(self: Box<Self>) {}",
            "a receiver is `&self`, `&mut self` or `self`",
        ),
    ];
    for (method, expected) in cases {
        let source = format!(
            "#[hxn::interface(shared)]\npub struct Thing;\n\n#[hxn::interface]\npub struct Plain;\n\n#[hxn::methods]\nimpl Thing {{\n    {method}\n}}\n"
        );
        let error = parse("thing", &source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{method}: {}",
            error.message
        );
        assert_eq!(error.line, 9, "{method}");
    }

    let clash = "#[hxn::interface(shared)]\npub struct Thing;\n\n#[hxn::interface]\npub struct ThingWeak;\n";
    let error = parse("thing", clash).unwrap_err();
    assert_eq!(error.line, 5);
    assert!(error.message.contains("is the weak reference to `Thing`"));
}

#[test]
fn bytes_come_back_with_their_length() {
    let source = "#[hxn::interface]\npub struct Disk;\n\n#[hxn::methods]\nimpl Disk {\n    pub fn read(&self, sector: i32) -> Result<Vec<u8>, String> { todo!() }\n}\n";
    let library = parse("store", source).unwrap();
    assert_eq!(library.interfaces[0].methods[0].returns, Type::Bytes);

    let c = hxn_gen::c_adapter(&library, "store_core");
    assert!(c.contains("out: *mut Buf, err: *mut Buf"));
    assert!(c.contains("Ok(Buf::from(this.read(sector)?))"));
    let glue = hxn_gen::cpp_glue(&library);
    assert!(glue.contains("inline Array<unsigned char> disk_read(void *self, int sector)"));
    assert!(glue.contains("return hxn::take_bytes(out);"));

    // HashLink's bytes do not carry a length, so it is written beside them.
    let hl = hxn_gen::hl_adapter(&library, "store_core");
    assert!(hl.contains("this: *mut Managed, sector: i32, out: *mut i32) -> *mut vbyte"));
    assert!(hl.contains("\"PXstore_disk_iRi_B\""));
    assert!(hl.contains("*out = i32::try_from(data.len()).map_err(|_| Fault::TooLarge)?;"));

    let haxe = &hxn_gen::haxe_hl(&library)[0].text;
    assert!(haxe.contains("disk_read(self:DiskRaw, sector:Int, out:hl.Ref<Int>):hl.Bytes"));
    assert!(haxe.contains("public function read(sector:Int):haxe.io.Bytes"));
    assert!(haxe.contains("return @:privateAccess new haxe.io.Bytes(data, outLength);"));
    let cpp = hxn_gen::haxe_cpp(&library);
    assert!(
        cpp[0]
            .text
            .contains("return haxe.io.Bytes.ofData(HxnStoreNative.disk_read(raw, sector));")
    );
    assert!(
        cpp.last()
            .unwrap()
            .text
            .contains("static function disk_read(self:Handle, sector:Int):haxe.io.BytesData;")
    );
    assert!(hxn_gen::manifest(&library).contains("\"returns\": \"bytes\""));
}

const LATER: &str = r#"
#[hxn::interface]
pub struct Radio;

#[hxn::interface(shared)]
pub struct Band;

#[hxn::methods]
impl Radio {
    pub fn tune(&self, station: &str) -> Future<i32> {
        todo!()
    }

    pub fn open(path: &str) -> Result<Future<Radio>, String> {
        todo!()
    }

    pub fn band(&self) -> Future<Arc<Band>> {
        todo!()
    }

    pub fn titles(&self) -> Stream<String> {
        todo!()
    }

    pub fn level(&self) -> Signal<f64> {
        todo!()
    }

    pub fn peak(&self) -> Signal<f64> {
        todo!()
    }
}
"#;

#[test]
fn results_over_time_are_handles_the_library_answers_for() {
    let library = parse("fm", LATER).unwrap();
    assert!(library.returns_later());
    let methods = &library.interfaces[0].methods;
    assert_eq!(methods[0].returns, Type::Future(Box::new(Type::I32)));
    assert_eq!(
        methods[1].returns,
        Type::Future(Box::new(handle("Radio", false)))
    );
    assert!(methods[1].fallible);
    assert_eq!(methods[3].returns, Type::Stream(Box::new(Type::Str)));
    assert_eq!(methods[4].returns, Type::Signal(Box::new(Type::F64)));

    // Handed over erased, and taken by what each carries. One export does
    // for every function that returns the same.
    let c = hxn_gen::c_adapter(&library, "fm_core");
    assert!(c.contains("use hxn_future::{AnyFuture, AnySignal, AnyStream, outbox};"));
    assert!(c.contains("Ok(Handle::into_raw(AnyFuture::from(this.tune(station.as_str()?))))"));
    assert!(c.contains("Ok(Handle::into_raw(AnyFuture::from(Radio::open(path.as_str()?)?)))"));
    assert!(c.contains("pub extern \"C\" fn fm_outbox_next() -> i32"));
    assert!(c.contains("pub unsafe extern \"C\" fn fm_future_wait(this: RawHandle, milliseconds: i32, out: *mut i32, err: *mut Buf) -> Status"));
    assert!(c.contains("this.take::<i32>()"));
    assert!(c.contains("Ok(Handle::into_raw(this.take::<Radio>()?))"));
    assert!(c.contains("Ok(Handle::into_raw(this.take::<Arc<Band>>()?))"));
    assert!(c.contains("Ok(Buf::from(this.next::<String>()?))"));
    assert!(c.contains("pub unsafe extern \"C\" fn fm_signal_wait(this: RawHandle, seen: i32, milliseconds: i32, out: *mut i32, err: *mut Buf) -> Status"));
    assert_eq!(c.matches("fn fm_signal_get_float(").count(), 1);

    let hl = hxn_gen::hl_adapter(&library, "fm_core");
    assert!(hl.contains("Ok(manage(AnyStream::from(this.titles())))"));
    assert!(hl.contains("\"PXfm_radio_OBi__Xfm_future_\""));
    assert!(hl.contains("define_prim!(hlp_outbox_wait, fm_outbox_wait, \"Pi_b\");"));
    assert!(hl.contains("define_prim!(hlp_future_wait, fm_future_wait, \"PXfm_future_i_i\");"));
    assert!(hl.contains("Ok(blocking(|| this.wait_millis(milliseconds)) as i32)"));
    assert!(hl.contains("define_prim!(hlp_signal_wait, fm_signal_wait, \"PXfm_signal_ii_i\");"));
    assert!(hl.contains(
        "define_prim!(hlp_future_take_radio, fm_future_take_radio, \"PXfm_future__Xfm_radio_\");"
    ));
    assert!(
        hl.contains("define_prim!(hlp_stream_next_text, fm_stream_next_text, \"PXfm_stream__B\");")
    );

    let header = hxn_gen::c_header(&library);
    assert!(header.contains("int32_t fm_outbox_wait(int32_t milliseconds);"));
    assert!(header.contains("hxn_status fm_signal_wait(hxn_handle *self, int32_t seen, int32_t milliseconds, int32_t *out, hxn_buf *err);"));
    assert!(header.contains(
        "hxn_status fm_future_take_band(hxn_handle *self, hxn_handle **out, hxn_buf *err);"
    ));
    let glue = hxn_gen::cpp_glue(&library);
    assert!(glue.contains("inline int signal_wait(void *self, int seen, int milliseconds)"));
    assert!(glue.contains("inline ::String stream_next_text(void *self)"));

    for files in [hxn_gen::haxe_hl(&library), cpp_classes(&library)] {
        let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "Radio.hx",
                "Band.hx",
                "BandWeak.hx",
                "HxnFmFuture.hx",
                "HxnFmStream.hx",
                "HxnFmSignal.hx",
                "HxnFmInbox.hx",
                "HxnFmMemory.hx"
            ]
        );
        let radio = &files[0].text;
        assert!(radio.contains("public function tune(station:String):hxn.Future<Int>"));
        assert!(radio.contains("public static function open(path:String):hxn.Future<Radio>"));
        assert!(radio.contains("public function titles():hxn.Stream<String>"));
        assert!(radio.contains("public function level():hxn.Signal<Float>"));
        let future = &files[3].text;
        for maker in ["ofInt", "ofRadio", "ofBand"] {
            assert!(future.contains(&format!("function {maker}(")), "{maker}");
        }
        assert!(future.contains("new hxn.Future(HxnFmInbox.inbox, "));
        assert!(
            files[5]
                .text
                .contains("hxn.Reactive.follow(HxnFmInbox.inbox, ")
        );
        assert!(
            files[6]
                .text
                .contains("public static final inbox = new hxn.Inbox(")
        );
    }
    let radio = &hxn_gen::haxe_hl(&library)[0].text;
    assert!(radio.contains("return HxnFmFuture.ofInt(Native.radio_tune(raw, station));"));
    let radio = &hxn_gen::haxe_cpp(&library)[0].text;
    assert!(radio.contains("var made = HxnFmStream.adopt();"));
    assert!(radio.contains("return made.ofText();"));

    let manifest = hxn_gen::manifest(&library);
    assert!(manifest.contains("\"returns\": \"future<Radio>\""));
    assert!(manifest.contains("\"returns\": \"signal<f64>\""));

    // A library with none of these names nothing of `hxn-future`.
    let plain = parse("audio", SOURCE).unwrap();
    assert!(!plain.returns_later());
    assert!(!hxn_gen::c_adapter(&plain, "audio_core").contains("hxn_future"));
    assert!(!hxn_gen::hl_adapter(&plain, "audio_core").contains("outbox"));
}

#[test]
fn says_what_a_result_over_time_cannot_carry() {
    let cases = [
        (
            "pub fn done(&self) -> Future<Vec<i32>> { todo!() }",
            "a future carries one of",
        ),
        (
            "pub fn nothing(&self) -> Signal<()> { todo!() }",
            "a signal carries one of",
        ),
        (
            "pub fn all(&self) -> Stream<Vec<i32>> { todo!() }",
            "a stream carries one of",
        ),
        (
            "pub fn now(&self) -> Signal<Thing> { todo!() }",
            "a signal carries one of",
        ),
        (
            "pub fn other(&self) -> Future<Band> { todo!() }",
            "carried in its `Arc`: `Future<Arc<Band>>`",
        ),
        (
            "pub fn boxed(&self) -> Stream<Arc<Thing>> { todo!() }",
            "needs `Thing` marked `#[hxn::interface(shared)]`",
        ),
        (
            "pub fn take(&self, later: Future<i32>) {}",
            "a parameter is one of",
        ),
    ];
    for (method, expected) in cases {
        let source = format!(
            "#[hxn::interface]\npub struct Thing;\n\n#[hxn::interface(shared)]\npub struct Band;\n\n#[hxn::methods]\nimpl Thing {{\n    {method}\n}}\n"
        );
        let error = parse("thing", &source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{method}: {}",
            error.message
        );
        assert_eq!(error.line, 9, "{method}");
    }
    for taken in ["Future", "Stream", "Signal", "Outbox"] {
        let source = format!("#[hxn::interface]\npub struct {taken};\n");
        assert!(
            parse("thing", &source)
                .unwrap_err()
                .message
                .contains("a name the adapters use")
        );
    }
}

#[test]
fn a_function_is_lent_for_the_call_and_can_fail() {
    let source = r#"
#[hxn::interface]
pub struct Mesh;

#[hxn::methods]
impl Mesh {
    pub fn each<E>(&mut self, scale: f64, mut f: impl FnMut(i32, f64) -> Result<bool, E>) -> Result<i32, E> {
        todo!()
    }

    pub fn tick<E>(f: impl Fn() -> Result<(), E>) -> Result<(), E> {
        f()
    }

    pub fn again<E>(&self, g: impl Fn() -> Result<(), E>) -> Result<(), E> {
        g()
    }
}
"#;
    let library = parse("geo", source).unwrap();
    let each = &library.interfaces[0].methods[0];
    assert_eq!(
        each.params[1].ty,
        Type::Lent {
            params: vec![Type::I32, Type::F64],
            returns: Box::new(Type::Bool),
        }
    );
    assert!(each.fallible);

    // One function pointer type and one trampoline a shape, however many
    // functions are lent it.
    let c = hxn_gen::c_adapter(&library, "geo_core");
    assert_eq!(c.matches("type LentIDToB = ").count(), 1);
    assert_eq!(c.matches("type LentToV = ").count(), 1);
    assert!(c.contains(
        "type LentIDToB = unsafe extern \"C\" fn(*mut c_void, i32, f64, *mut i32) -> Status;"
    ));
    assert!(c.contains("scale: f64, f: Callback<LentIDToB>, out: *mut i32"));
    assert!(c.contains("this.each(scale, |a0, a1| f.invoke(a0, a1).map(|out| out != 0))?"));
    assert!(c.contains("Mesh::tick(|| f.invoke().map(|_| ()))?"));

    let hl = hxn_gen::hl_adapter(&library, "geo_core");
    assert!(hl.contains("\"PXgeo_mesh_dPid_Nb_i\""));
    assert!(hl.contains("\"PP_b_v\""));
    assert!(hl.contains("let f = Callback::from_raw(lent_id_to_b as LentIDToB, f.cast(), None);"));
    assert!(hl.contains("unsafe extern \"C\" fn lent_id_to_b(env: *mut c_void, a0: i32, a1: f64, out: *mut i32) -> Status"));
    assert!(hl.contains("let mut args: [*mut vdynamic; 2] = [box_i32(a0), box_f64(a1)];"));
    assert_eq!(hl.matches("fn lent_to_v(").count(), 1);

    let header = hxn_gen::c_header(&library);
    assert!(header.contains("hxn_status geo_mesh_each(hxn_handle *self, double scale, hxn_callback f, int32_t *out, hxn_buf *err);"));
    let glue = hxn_gen::cpp_glue(&library);
    assert!(
        glue.contains("inline hxn_status lent_id_to_b(void *env, int a0, double a1, int *out)")
    );
    assert!(glue.contains("inline int mesh_each(void *self, double scale, Dynamic f)"));
    assert!(glue.contains("hxn_callback f_lent = { (void *)lent_id_to_b, f.mPtr, 0 };"));

    // The Haxe method wraps what it was passed and calls inside `during`.
    for files in [hxn_gen::haxe_hl(&library), cpp_classes(&library)] {
        let mesh = &files[0].text;
        assert!(mesh.contains("public function each(scale:Float, f:(Int, Float) -> Bool):Int {"));
        assert!(mesh.contains("var fGuarded = function(a0:Int, a1:Float):Null<Bool> {"));
        assert!(mesh.contains("hxn.Callback.failed(error);"));
        assert!(mesh.contains("return hxn.Callback.during(() -> {"));
        assert!(mesh.contains("public static function tick(f:() -> Void):Void {"));
        assert!(mesh.contains("var fGuarded = function():Bool {"));
        assert!(mesh.contains("\t\t\treturn true;\n\t\t});"));
    }
    let mesh = &hxn_gen::haxe_hl(&library)[0].text;
    assert!(
        mesh.contains("mesh_each(self:MeshRaw, scale:Float, f:(Int, Float) -> Null<Bool>):Int")
    );
    assert!(hxn_gen::manifest(&library).contains("\"type\": \"fn(i32, f64) -> bool\""));
}

#[test]
fn says_how_a_function_is_lent() {
    let cases = [
        (
            "pub fn a(&self, f: impl Fn(i32) -> i32) {}",
            "is lent as `impl Fn(..) -> Result<T, E>`",
        ),
        (
            "pub fn b<E>(&self, f: impl Fn(&[u8]) -> Result<i32, E>) {}",
            "is lent as",
        ),
        (
            "pub fn c<E>(&self, f: impl Fn(i32) -> Result<&str, E>) {}",
            "is lent as",
        ),
        (
            "pub fn j<E>(&self, f: impl Fn(String) -> Result<i32, E>) {}",
            "is lent as",
        ),
        (
            "pub fn d(&self, f: impl Iterator<Item = i32>) {}",
            "is lent as",
        ),
        (
            "pub fn e(&self, f: &dyn Fn(i32) -> i32) {}",
            "a parameter is one of",
        ),
        (
            "#[hxn(blocking)]\n    pub fn g<E>(&self, f: impl Fn() -> Result<(), E>) {}",
            "a function that waits cannot be lent",
        ),
        (
            "pub fn h<T>(&self, value: i32) {}",
            "a generic function cannot cross",
        ),
        (
            "pub fn i<'a, E>(&self, f: impl Fn() -> Result<(), E>) {}",
            "a generic function cannot cross",
        ),
    ];
    for (method, expected) in cases {
        let (line, message) = refused(method);
        assert!(message.contains(expected), "{method}: {message}");
        assert!((6..=7).contains(&line), "{method}: line {line}");
    }
}

// A library laid out the ways a crate can be: a module beside the root, one
// in a directory, one inside an inline module, and one at a path given.
#[test]
fn a_library_is_read_from_every_file_of_its_crate() {
    let dir = std::env::temp_dir().join(format!("hxn-gen-crate-{}", std::process::id()));
    let write = |path: &str, text: &str| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        "src/lib.rs",
        "mod beside;\nmod within;\nmod inline {\n    pub mod deep;\n}\n#[path = \"odd/place.rs\"]\nmod elsewhere;\n\n#[hxn::interface]\npub struct Root;\n",
    );
    write(
        "src/beside.rs",
        "mod child;\n\n#[hxn::interface]\npub struct Beside;\n\nuse crate::Root;\n\n#[hxn::methods]\nimpl Root {\n    pub fn depth(&self) -> i32 { 0 }\n}\n\n#[hxn::methods]\nimpl Beside {\n    pub fn root(&self, root: &Root) -> i32 { 0 }\n}\n",
    );
    write(
        "src/beside/child.rs",
        "#[hxn::interface]\npub struct Child;\n",
    );
    write(
        "src/within/mod.rs",
        "#[hxn::interface]\npub struct Within;\n",
    );
    write(
        "src/inline/deep.rs",
        "#[hxn::interface]\npub struct Deep;\n",
    );
    write(
        "src/odd/place.rs",
        "#[hxn::interface]\npub struct Elsewhere;\n\n#[hxn::methods]\nimpl Elsewhere {\n    pub fn bad(&self, value: String) {}\n}\n",
    );

    // A refusal names the file it is in.
    let error = hxn_gen::parse_crate("many", &dir.join("src/lib.rs")).unwrap_err();
    assert!(error.file.as_deref().unwrap().ends_with("place.rs"));
    assert_eq!(error.line, 6);
    assert!(
        error
            .to_string()
            .contains("place.rs:6: a parameter is one of")
    );

    write(
        "src/odd/place.rs",
        "#[hxn::interface]\npub struct Elsewhere;\n",
    );
    let (library, files) = hxn_gen::parse_crate("many", &dir.join("src/lib.rs")).unwrap();
    let names: Vec<_> = library.interfaces.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        names,
        ["Root", "Beside", "Child", "Within", "Deep", "Elsewhere"]
    );
    assert_eq!(files.len(), 6);
    // A block in one file exposes functions on, and names, an interface of another.
    let beside = &library.interfaces[1];
    assert_eq!(beside.methods[0].params[0].ty, handle("Root", false));
    assert_eq!(library.interfaces[0].methods[0].name, "depth");

    write("src/within/mod.rs", "#[hxn::interface]\npub struct Root;\n");
    let error = hxn_gen::parse_crate("many", &dir.join("src/lib.rs")).unwrap_err();
    assert!(
        error
            .message
            .contains("`Root` is marked as an interface twice")
    );

    std::fs::remove_file(dir.join("src/within/mod.rs")).unwrap();
    let error = hxn_gen::parse_crate("many", &dir.join("src/lib.rs")).unwrap_err();
    assert!(
        error
            .message
            .contains("the file of module `within` was not found")
    );
    assert!(error.file.as_deref().unwrap().ends_with("lib.rs") && error.line == 2);

    std::fs::remove_dir_all(&dir).unwrap();
}

// The fixture's manifest is committed, as a library's would be, so a change
// to what it exposes shows up in review. To update it after changing
// tests/counter/core, run hxn-gen and copy its counter.hxn.json over.
#[test]
fn the_counter_manifest_is_current() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/counter");
    let committed = std::fs::read_to_string(format!("{root}/counter.hxn.json")).unwrap();
    let (library, files) =
        hxn_gen::parse_crate("counter", format!("{root}/core/src/lib.rs").as_ref()).unwrap();
    assert!(
        files.len() > 1,
        "the fixture keeps a module in its own file"
    );
    assert_eq!(hxn_gen::manifest(&library), committed);
    assert_eq!(
        hxn_gen::manifest(&library),
        hxn_gen::manifest(&library.clone())
    );
}

// Each example is a library as someone would write one: its manifest is
// committed and current, and it records what its program prints. To update a
// manifest, run the example.
#[test]
fn every_example_is_one_the_generator_takes() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut seen = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_str().unwrap();
        let (library, _) = hxn_gen::parse_crate(name, &dir.join("src/lib.rs")).unwrap();
        let committed = std::fs::read_to_string(dir.join(format!("{name}.hxn.json"))).unwrap();
        assert_eq!(hxn_gen::manifest(&library), committed, "{name}");
        assert!(dir.join("haxe/Main.hx").is_file(), "{name}");
        assert!(dir.join("expected.txt").is_file(), "{name}");
        seen += 1;
    }
    assert!(seen >= 3);
}

#[test]
fn a_number_crosses_as_one_of_the_two_the_boundary_has() {
    let source = "#[hxn::interface]\npub struct Gauge;\n\n#[hxn::methods]\nimpl Gauge {\n    pub fn set(&mut self, level: u8, gain: f32) {}\n    pub fn mask(&self, bits: u32) -> u32 { todo!() }\n    pub fn len(&self) -> usize { todo!() }\n    pub fn seek(&mut self, by: isize) -> Result<i16, String> { todo!() }\n}\n";
    let library = parse("meter", source).unwrap();
    let methods = &library.interfaces[0].methods;
    assert_eq!(methods[0].params[0].ty, Type::I32);
    assert_eq!(methods[0].params[0].number, Some(Number::U8));
    assert_eq!(methods[0].params[1].ty, Type::F64);
    assert_eq!(methods[1].returns_number, Some(Number::U32));
    assert_eq!(methods[3].returns, Type::I32);
    assert_eq!(methods[3].returns_number, Some(Number::I16));

    // Nothing of the boundary changes: the adapters convert on the way.
    let header = hxn_gen::c_header(&library);
    assert!(
        header.contains(
            "meter_gauge_set(hxn_handle *self, int32_t level, double gain, hxn_buf *err);"
        )
    );
    for adapter in [
        hxn_gen::c_adapter(&library, "meter_core"),
        hxn_gen::hl_adapter(&library, "meter_core"),
    ] {
        assert!(
            adapter
                .contains("this.set(u8::try_from(level).map_err(|_| Fault::Range)?, gain as f32)")
        );
        assert!(adapter.contains("Ok(this.mask(bits as u32) as i32)"));
        assert!(adapter.contains("Ok(i32::try_from(this.len()).map_err(|_| Fault::Range)?)"));
        assert!(
            adapter.contains(
                "Ok(i32::from(this.seek(isize::try_from(by).map_err(|_| Fault::Range)?)?))"
            )
        );
    }

    // Haxe sees an Int or a Float, and a UInt for the unsigned 32 bits.
    for files in [hxn_gen::haxe_hl(&library), cpp_classes(&library)] {
        let gauge = &files[0].text;
        assert!(gauge.contains("public function set(level:Int, gain:Float):Void"));
        assert!(gauge.contains("public function mask(bits:UInt):UInt"));
        assert!(gauge.contains("public function len():Int"));
    }
    let manifest = hxn_gen::manifest(&library);
    assert!(manifest.contains("{ \"name\": \"level\", \"type\": \"u8\" }"));
    assert!(manifest.contains("\"returns\": \"usize\""));
}

#[test]
fn sixty_four_bits_cross_as_themselves() {
    let source = "#[hxn::interface]\npub struct Clock;\n\n#[hxn::methods]\nimpl Clock {\n    pub fn at(&self, nanos: i64) -> Result<i64, String> { todo!() }\n    pub fn id(&self, salt: u64) -> u64 { todo!() }\n}\n";
    let library = parse("time", source).unwrap();
    let methods = &library.interfaces[0].methods;
    assert_eq!(methods[0].params[0].ty, Type::I64);
    assert_eq!(methods[0].returns, Type::I64);
    assert_eq!(methods[1].params[0].number, Some(Number::U64));

    let header = hxn_gen::c_header(&library);
    assert!(
        header.contains(
            "time_clock_at(hxn_handle *self, int64_t nanos, int64_t *out, hxn_buf *err);"
        )
    );
    let glue = hxn_gen::cpp_glue(&library);
    assert!(glue.contains("inline cpp::Int64 clock_at(void *self, cpp::Int64 nanos)"));
    let c = hxn_gen::c_adapter(&library, "time_core");
    assert!(c.contains("nanos: i64, out: *mut i64, err: *mut Buf"));
    assert!(c.contains("Ok(this.id(salt as u64) as i64)"));
    let hl = hxn_gen::hl_adapter(&library, "time_core");
    assert!(hl.contains("this: *mut Managed, nanos: i64) -> i64"));
    assert!(hl.contains("\"PXtime_clock_l_l\""));

    let hl = &hxn_gen::haxe_hl(&library)[0].text;
    assert!(hl.contains("clock_at(self:ClockRaw, nanos:hl.I64):hl.I64"));
    let cpp = hxn_gen::haxe_cpp(&library);
    assert!(
        cpp.last()
            .unwrap()
            .text
            .contains("static function clock_at(self:Handle, nanos:cpp.Int64):cpp.Int64;")
    );
    for files in [hxn_gen::haxe_hl(&library), cpp_classes(&library)] {
        let clock = &files[0].text;
        assert!(clock.contains("public function at(nanos:haxe.Int64):haxe.Int64"));
        assert!(clock.contains("public function id(salt:haxe.Int64):haxe.Int64"));
    }
    let manifest = hxn_gen::manifest(&library);
    assert!(manifest.contains("{ \"name\": \"nanos\", \"type\": \"i64\" }"));
    assert!(manifest.contains("\"returns\": \"u64\""));
}

const FUNCTIONS: &str = r#"
#[hxn::interface]
pub struct Shelf;

#[hxn::methods]
impl Shelf {
    pub fn count(&self) -> i32 { 0 }
}

/// How many shelves a library may have.
#[hxn::function]
pub fn limit() -> i32 { 8 }

#[hxn::function(class = "Shelf")]
pub fn merged(first: &Shelf, second: &Shelf) -> Shelf { todo!() }

#[hxn::function(class = "Paths")]
pub fn join(first: &str, second: &str) -> Result<String, String> { todo!() }

pub fn unmarked() {}
"#;

#[test]
fn a_function_outside_an_impl_is_a_static_of_a_class() {
    let library = parse("book_store", FUNCTIONS).unwrap();
    let names: Vec<_> = library
        .interfaces
        .iter()
        .map(|interface| (interface.name.as_str(), interface.namespace))
        .collect();
    // The library's own class and one named on the attribute exist only to
    // hold functions; the third joins an interface that is there.
    assert_eq!(
        names,
        [("Shelf", false), ("BookStore", true), ("Paths", true)]
    );
    let shelf = &library.interfaces[0];
    assert_eq!(shelf.methods.len(), 2);
    assert!(!shelf.methods[0].free && shelf.methods[1].free);
    assert_eq!(
        library.interfaces[1].methods[0].docs,
        ["How many shelves a library may have."]
    );

    // An adapter finds them at the crate's root, by a path.
    for adapter in [
        hxn_gen::c_adapter(&library, "store_core"),
        hxn_gen::hl_adapter(&library, "store_core"),
    ] {
        assert!(adapter.contains("use store_core as hxn_library;"));
        assert!(adapter.contains("use store_core::Shelf;"));
        assert!(adapter.contains("Ok(hxn_library::limit())"));
        assert!(adapter.contains("hxn_library::merged(&first, &second)"));
        assert!(adapter.contains("hxn_library::join("));
        assert!(adapter.contains("fn book_store_paths_join("));
        // Nothing is made of such a class, so nothing closes one.
        assert!(adapter.contains("fn book_store_shelf_close("));
        assert!(!adapter.contains("book_store_paths_close"));
        assert!(!adapter.contains("book_store_book_store_close"));
    }
    let header = hxn_gen::c_header(&library);
    assert!(header.contains("hxn_status book_store_book_store_limit(int32_t *out, hxn_buf *err);"));
    assert!(!header.contains("book_store_paths_close"));
    let glue = hxn_gen::cpp_glue(&library);
    assert!(glue.contains("inline int book_store_limit()"));
    assert!(!glue.contains("paths_close"));

    for files in [hxn_gen::haxe_hl(&library), cpp_classes(&library)] {
        let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths[..3], ["Shelf.hx", "BookStore.hx", "Paths.hx"]);
        assert!(
            files[0]
                .text
                .contains("public static function merged(first:Shelf, second:Shelf):Shelf")
        );
        let store = &files[1].text;
        assert!(store.contains("class BookStore {"));
        assert!(store.contains("/** How many shelves a library may have. **/"));
        assert!(store.contains("public static function limit():Int"));
        for part in ["raw", "adopt", "close", "finalize"] {
            assert!(!store.contains(part), "{part}");
        }
        assert!(
            files[2]
                .text
                .contains("public static function join(first:String, second:String):String")
        );
    }
    let native = hxn_gen::haxe_cpp(&library).pop().unwrap().text;
    assert!(native.contains("static function paths_join(first:String, second:String):String;"));
    assert!(!native.contains("paths_close"));

    let manifest = hxn_gen::manifest(&library);
    assert_eq!(manifest.matches("\"namespace\": true").count(), 2);
}

#[test]
fn says_what_a_function_outside_an_impl_cannot_be() {
    let cases = [
        ("#[hxn::function]\nfn hidden() {}", "has to be `pub`"),
        (
            "#[hxn::function]\npub fn new() -> Thing { todo!() }",
            "has another name",
        ),
        (
            "#[hxn::function(class = \"Thing\")]\npub fn size() -> i32 { 0 }",
            "`Thing` already has a function named `size`",
        ),
        (
            "#[hxn::function]\npub fn twice() {}\n#[hxn::function]\npub fn twice() {}",
            "already has a function named `twice`",
        ),
        (
            "#[hxn::function(class = \"Thing\")]\npub fn close() {}",
            "`close` is what releases the value",
        ),
        (
            "#[hxn::function(class = \"tools\")]\npub fn a() {}",
            "the name a Haxe class can have",
        ),
        (
            "#[hxn::function(on = \"Tools\")]\npub fn b() {}",
            "expected `#[hxn::function]`",
        ),
        (
            "#[hxn::function(class = \"Memory\")]\npub fn c() {}",
            "a name the adapters use",
        ),
        ("#[hxn::function]\npub fn d<T>(value: T) {}", "generic"),
    ];
    for (function, expected) in cases {
        let source = format!(
            "#[hxn::interface]\npub struct Thing;\n\n#[hxn::methods]\nimpl Thing {{\n    pub fn size(&self) -> i32 {{ 0 }}\n}}\n\n{function}\n"
        );
        let error = parse("thing", &source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{function}: {}",
            error.message
        );
    }
    // A library may be functions and nothing else.
    let only = parse(
        "maths",
        "#[hxn::function]\npub fn double(value: i32) -> i32 { value * 2 }\n",
    )
    .unwrap();
    assert_eq!(only.interfaces[0].name, "Maths");
    assert!(hxn_gen::hl_adapter(&only, "maths").contains("Ok(hxn_library::double(value))"));
}

// The public functions a class file has, as they are declared.
fn public_functions(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.starts_with("\tpublic ") && line.contains("function "))
        .map(|line| {
            line.trim_end_matches(" {")
                .trim_end_matches(';')
                .trim()
                .to_owned()
        })
        .collect()
}

// A script calls the classes its host holds, so each declaration it is
// compiled against has to list what the hxcpp class of that name has.
#[test]
fn a_script_is_told_what_each_class_is_called_with() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/counter");
    let (library, _) =
        hxn_gen::parse_crate("counter", format!("{root}/core/src/lib.rs").as_ref()).unwrap();
    let held = hxn_gen::haxe_cpp(&library);
    let declared = hxn_gen::haxe_cppia(&library);
    for interface in &library.interfaces {
        let path = format!("{}.hx", interface.name);
        assert!(declared.iter().any(|file| file.path == path), "{path}");
    }
    for declaration in &declared {
        let class = held
            .iter()
            .find(|file| file.path == declaration.path)
            .unwrap_or_else(|| panic!("{} declares a class no host holds", declaration.path));
        assert!(
            declaration.text.contains("\nextern class "),
            "{}",
            declaration.path
        );
        assert_eq!(
            public_functions(&declaration.text),
            public_functions(&class.text),
            "{}",
            declaration.path
        );
        // Nothing of how the class reaches the library.
        for inside in ["raw", "Native", "cpp."] {
            assert!(
                !declaration.text.contains(inside),
                "{} names `{inside}`",
                declaration.path
            );
        }
    }
    let memory = declared
        .iter()
        .find(|file| file.path == "HxnCounterMemory.hx")
        .unwrap();
    assert!(
        memory
            .text
            .contains("public static var meter(default, never):hxn.Gc.Meter;")
    );
}

// An async function runs after the call that starts it has returned: the
// host gets a future, and the work owns everything it was passed.
#[test]
fn an_async_function_is_a_future_of_what_it_gives() {
    let source = r#"
#[hxn::interface]
pub struct Thing;

#[hxn::interface(shared)]
pub struct Band;

#[hxn::methods]
impl Thing {
    pub async fn count(limit: u8, name: &str, data: &[u8]) -> Result<i32, String> { todo!() }
    pub async fn rest() {}
    pub async fn spend(self) -> String { todo!() }
}

#[hxn::methods]
impl Band {
    pub async fn play(&self, with: &Band, after: Option<Arc<Band>>) -> Arc<Band> { todo!() }
}
"#;
    let library = parse("thing", source).unwrap();
    let thing = &library.interfaces[0];
    let count = &thing.methods[0];
    assert!(count.asynchronous && count.fallible);
    assert_eq!(count.returns, Type::Future(Box::new(Type::I32)));
    assert_eq!(count.returns_number, None);
    let rest = &thing.methods[1];
    assert!(rest.asynchronous && !rest.fallible);
    assert_eq!(rest.returns, Type::Future(Box::new(Type::Void)));

    let manifest = hxn_gen::manifest(&library);
    assert_eq!(manifest.matches("\"async\": true").count(), 4);
    assert!(!manifest.contains("\"fallible\": true"), "{manifest}");
    assert!(manifest.contains("\"returns\": \"future<void>\""));

    for adapter in [
        hxn_gen::c_adapter(&library, "thing"),
        hxn_gen::hl_adapter(&library, "thing"),
    ] {
        // Converted, and refused if it does not fit, before the work starts.
        assert!(
            adapter.contains("let limit = u8::try_from(limit)"),
            "{adapter}"
        );
        assert!(adapter.contains("let name = String::from("));
        assert!(adapter.contains("let data = <[u8]>::to_vec("));
        assert!(
            adapter.contains(
                "hxn_future::spawn(async move { Thing::count(limit, &name, &data).await })"
            )
        );
        assert!(adapter.contains(
            "hxn_future::spawn(async move { Thing::rest().await; Ok::<_, hxn_future::Failure>(()) })"
        ));
        assert!(adapter.contains("Ok::<_, hxn_future::Failure>(this.spend().await)"));
        // The object is taken out, so it is closed whatever the work does.
        assert!(adapter.contains(".take()?.ok_or(Fault::Closed)?;"));
        assert!(adapter.contains("this.spend().await"));
        // A share each, and none where there was no object.
        assert!(adapter.contains("let this = Arc::clone(&this);"));
        assert!(adapter.contains("let with = Arc::clone(&with);"));
        assert!(adapter.contains("let after = after.as_deref().cloned();"));
        assert!(adapter.contains("this.play(&with, after).await"));
    }
    for class in [
        &hxn_gen::haxe_hl(&library)[0],
        &cpp_classes(&library)[0],
        &hxn_gen::haxe_cppia(&library)[0],
    ] {
        assert!(
            class
                .text
                .contains("public static function count(limit:Int, name:String, data:haxe.io.Bytes):hxn.Future<Int>"),
            "{}",
            class.text
        );
        assert!(
            class
                .text
                .contains("public static function rest():hxn.Future<hxn.Unit>")
        );
        assert!(
            class
                .text
                .contains("public function spend():hxn.Future<String>")
        );
    }

    let refused = [
        ("pub async fn new() -> Self { Self }", "`new` is not async"),
        (
            "pub async fn read(&self) {}",
            "cannot borrow the object: take `self`, or mark the interface `shared`",
        ),
        (
            "pub async fn write(&mut self) {}",
            "cannot borrow the object",
        ),
        (
            "pub async fn with(other: &Thing) {}",
            "cannot take an object, which is lent for the call: `other`",
        ),
        (
            "pub async fn tune(other: &mut Thing) {}",
            "cannot take an object",
        ),
        (
            "pub async fn watch(cell: &AtomicI32) {}",
            "cannot take a cell",
        ),
        (
            "pub async fn each<E>(f: impl Fn(i32) -> Result<i32, E>) {}",
            "cannot take a function",
        ),
        (
            "#[hxn(blocking)]\n    pub async fn wait() {}",
            "an async function does not wait",
        ),
        (
            "pub async fn twice() -> Future<i32> { todo!() }",
            "return the value, not a Future of it",
        ),
        (
            "pub async fn band() -> Band { todo!() }",
            "given in its `Arc`: `Arc<Band>`",
        ),
        ("pub async fn wide() -> u8 { 0 }", "a future carries one of"),
    ];
    for (method, expected) in refused {
        let source = format!(
            "#[hxn::interface]\npub struct Thing;\n\n#[hxn::interface(shared)]\npub struct Band;\n\n#[hxn::methods]\nimpl Thing {{\n    {method}\n}}\n"
        );
        let error = parse("thing", &source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{method}: {}",
            error.message
        );
    }
}

#[test]
fn a_result_over_time_carries_bytes_and_sixty_four_bits() {
    let source = r#"
#[hxn::interface]
pub struct Thing;

#[hxn::methods]
impl Thing {
    pub fn stamp() -> Future<i64> { todo!() }
    pub fn frames() -> Stream<Vec<u8>> { todo!() }
    pub fn count() -> Signal<i64> { todo!() }
}
"#;
    let library = parse("thing", source).unwrap();
    let methods = &library.interfaces[0].methods;
    assert_eq!(methods[0].returns, Type::Future(Box::new(Type::I64)));
    assert_eq!(methods[1].returns, Type::Stream(Box::new(Type::Bytes)));
    assert_eq!(methods[2].returns, Type::Signal(Box::new(Type::I64)));

    let c = hxn_gen::c_adapter(&library, "thing");
    assert!(c.contains("fn thing_future_take_long(this: RawHandle, out: *mut i64, err: *mut Buf)"));
    assert!(
        c.contains("fn thing_stream_next_bytes(this: RawHandle, out: *mut Buf, err: *mut Buf)")
    );
    let header = hxn_gen::c_header(&library);
    assert!(
        header.contains("thing_future_take_long(hxn_handle *self, int64_t *out, hxn_buf *err);")
    );
    assert!(
        header.contains("thing_signal_get_long(hxn_handle *self, int64_t *out, hxn_buf *err);")
    );
    // HashLink's bytes come with their length through a last parameter.
    let hl = hxn_gen::hl_adapter(&library, "thing");
    assert!(
        hl.contains("fn thing_stream_next_bytes(this: *mut Managed, out: *mut i32) -> *mut vbyte")
    );
    assert!(hl.contains("\"PXthing_stream_Ri_B\""), "{hl}");
    assert!(hl.contains("\"PXthing_future__l\""));
    let class = &hxn_gen::haxe_hl(&library)[0];
    assert!(
        class
            .text
            .contains("public static function frames():hxn.Stream<haxe.io.Bytes>")
    );
    assert!(
        class
            .text
            .contains("public static function count():hxn.Signal<haxe.Int64>")
    );

    // A signal is read any number of times, and bytes are handed over once.
    let refused = source.replace("Signal<i64>", "Signal<Vec<u8>>");
    assert!(
        parse("thing", &refused)
            .unwrap_err()
            .message
            .contains("a signal carries one of")
    );
}

// Text is lent to a Haxe function for the call and comes back as the
// library's own.
#[test]
fn a_lent_function_takes_and_gives_text() {
    let source = r#"
#[hxn::interface]
pub struct Thing;

#[hxn::methods]
impl Thing {
    pub fn name<E>(&self, f: impl Fn(&str, bool) -> Result<String, E>) -> Result<String, E> { todo!() }
}
"#;
    let library = parse("thing", source).unwrap();
    assert_eq!(
        library.interfaces[0].methods[0].params[0].ty,
        Type::Lent {
            params: vec![Type::Str, Type::Bool],
            returns: Box::new(Type::Str),
        }
    );
    for adapter in [
        hxn_gen::c_adapter(&library, "thing"),
        hxn_gen::hl_adapter(&library, "thing"),
    ] {
        assert!(adapter.contains(
            "type LentSBToS = unsafe extern \"C\" fn(*mut c_void, hxn_abi::Slice<'_>, i32, *mut hxn_abi::Buf) -> Status;"
        ));
        assert!(adapter.contains(
            "unsafe fn call_sb_to_s(callback: &Callback<LentSBToS>, a0: &str, a1: bool) -> Result<String, hxn_abi::Failure>"
        ));
        assert!(adapter.contains("this.name(|a0, a1| call_sb_to_s(&f, a0, a1))"));
    }
    let hl = hxn_gen::hl_adapter(&library, "thing");
    assert!(hl.contains("let Ok(a0) = a0.as_str() else {"));
    assert!(hl.contains("box_bytes(utf16(a0)), box_bool(a1 != 0)"));
    assert!(hl.contains("PBb_OBi_"), "{hl}");
    let glue = hxn_gen::cpp_glue(&library);
    assert!(
        glue.contains(
            "inline hxn_status lent_sb_to_s(void *env, hxn_slice a0, int a1, hxn_buf *out)"
        )
    );
    assert!(glue.contains("Dynamic((hx::Object *)env)(hxn::text(a0), a1 != 0)"));
    assert!(glue.contains("hxn::give_text((::String)result, out)"));

    // HashLink's wrapper is handed the VM's bytes and makes the string.
    let hl_class = &hxn_gen::haxe_hl(&library)[0].text;
    assert!(hl_class.contains("public function name(f:(String, Bool) -> String):String"));
    assert!(hl_class.contains("f:(hl.Bytes, Bool) -> Null<String>"));
    assert!(hl_class.contains("f(@:privateAccess String.fromUCS2(a0), a1)"));
    let cpp_class = &cpp_classes(&library)[0].text;
    assert!(cpp_class.contains("var fGuarded = function(a0:String, a1:Bool):Null<String>"));
    assert!(cpp_class.contains("a function lent to a library returned null"));
}
