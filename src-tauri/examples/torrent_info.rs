use lava_torrent::torrent::v1::Torrent;

fn main() {
    for path in std::env::args().skip(1) {
        match Torrent::read_from_file(&path) {
            Ok(t) => println!(
                "path={path}\nname={}\ninfo_hash={}\nannounce={:?}\nannounce_list={:?}\nsize={}\nextra_info={:?}\n",
                t.name, t.info_hash(), t.announce, t.announce_list, t.length, t.extra_info_fields
            ),
            Err(error) => eprintln!("path={path}\nerror={error}"),
        }
    }
}
