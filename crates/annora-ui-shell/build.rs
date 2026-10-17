#[cfg(feature = "render")]
fn main() {
    slint_build::compile("ui/annora.slint").expect("Slint compile failed");
}

#[cfg(not(feature = "render"))]
fn main() {}
