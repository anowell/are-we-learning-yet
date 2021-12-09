
scrape:
 cd _scraper && cargo build
 _scraper/target/debug/scraper _data/crates.yaml

clean:
  rm _data/crates_generated.yaml
  rm -rf _tmp
