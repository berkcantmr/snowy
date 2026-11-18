fn main() {
    snowy_sys::init_dpi_awareness();
    let t = std::time::Instant::now();
    match snowy_sys::query_desktop_icons() {
        Ok(rects) => {
            println!("query took {:?}", t.elapsed());
            for r in &rects {
                println!("{} {} {} {}", r.0, r.1, r.2, r.3);
            }
            println!("count: {}", rects.len());
        }
        Err(e) => println!("error after {:?}: {e:#}", t.elapsed()),
    }
}
