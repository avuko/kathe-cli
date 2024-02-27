use clap::{Arg, Command, ArgGroup, ArgAction};
use md5::Md5;
use redis::{Client, Commands, Connection, RedisResult};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io, path::Path};
use std::error::Error;

// XXX This tool is currently very naive, because checking every input for valid formatting
// over millions of rows is expensive. I might add some checks to "read_tsv" eventually.
// For now, it only validates if the record is the same length as the one before and errors out if
// not.
//
// Structure of command line arguments

/// Command line tool to fill the kathe redis store
/// input: command line arguments
/// output: file metadata stored in redis, or TSV records to STDOUT
fn main() {
    let matches = Command::new("kathe")
        .author("avuko")
        .version("0.5")
        .about("kathe is a tool to correlate inputs based on ssdeep similarity
TSV fields: \"inputname\"\\t\"md5\"\\t\"sha1\"\\t\"sha256\"\\t\"ssdeep\"\\t\"context[,context,...]\"
named after Katherine Johnson of NASA fame.")
        .arg(Arg::new("dbnumber").short('d').long("dbnumber").takes_value(true).default_value("7"))
        .arg(Arg::new("redishost").short('r').long("redishost").takes_value(true).default_value("127.0.0.1"))
        .arg(Arg::new("port").short('p').long("port").default_value("6379"))
        .arg(Arg::new("auth").short('a').long("auth").default_value("redis"))
        .arg(Arg::new("context").short('c').long("context").required(true).takes_value(true).help("list,of,contexts"))
        .arg(Arg::new("inputtsv").short('i').long("inputtsv").takes_value(false).help("Parse a TSV from STDIN").action(ArgAction::SetTrue).group("input"))
        .arg(Arg::new("filepath").short('f').long("filepath").takes_value(true).help("Path to file to be parsed").group("input"))
        .group(ArgGroup::with_name("input")
               .args(&["filepath","inputtsv"])
               .required(true)
               .multiple(true))
        .get_matches();          

    // sanitize and create context array
    let contextarg = matches.get_one::<String>("context").unwrap(); 
    let context = make_context(&contextarg);

    // If we DO have an input switch, we read from STDIN and store in redis 
    if matches.get_flag("inputtsv") {
        eprintln!("Request to parse input from STDIN found");
        // XXX DEEPLY naive implementation

        // redis settings
        // sanity checking redisdb
        let dbnumberarg = matches.get_one::<String>("dbnumber").unwrap(); 
        let redisdb = match dbnumberarg.trim().parse::<i32>() {
            Ok(dbnumber) => dbnumber,
            Err(_) => 7,
        };

        // sanity checking redishost
        let redishostarg = matches.get_one::<String>("redishost").unwrap();
        let redishost = match redishostarg.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => "127.0.0.1".parse::<IpAddr>().unwrap(),
        };

        // sanity checking redisport
        let portarg = matches.get_one::<String>("port").unwrap(); 
        let redisport = match portarg.trim().parse::<i32>() {
            Ok(port) => port,
            Err(_) => 6379,
        };

        // store redis auth keyword 
        let autharg = matches.get_one::<String>("auth").unwrap(); 
        let redispassword = autharg.to_string();

        // create a client to connect to
        let mut client = connect(redispassword, redishost, redisport, redisdb);

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .comment(Some(b'#'))
            .delimiter(b'\t')        
            .flexible(false)
            .from_reader(io::stdin());
        for result in rdr.records() {
            match result {
                // print error to STDERR but continue
                Err(error) => eprintln!("{:?}", error),
                Ok(record) => {
                    let inputname = &record[0].to_string();
                    let md5output = &record[1].to_string();
                    let sha1output = &record[2].to_string();
                    let sha256output = &record[3].to_string();
                    let ssdeepoutput = &record[4].to_string();
                    let context = &make_context(&record[5].to_string());
                    let _: RedisResult<()> = add_data(
                        &mut client,
                        &inputname,
                        &md5output,
                        &sha1output,
                        &sha256output,
                        &ssdeepoutput,
                        &context,
                        );
                }
            }
        }
        if let Ok(timestamp) = get_timestamp(&mut client) {
        eprintln!("current timestamp: {}", timestamp);
        }
        // IF we DON'T have an input switch, we read a file and print TSV formatted records to STDOUT
    } else {
        // get and check file(path)
        let filepatharg = matches.get_one::<String>("filepath").unwrap();
        let filepath = filepatharg.to_string();
        // let filepath = String::from(filepath);
        check_file(&filepath);

        // sanitize and create inputname
        let inputname = remove_badchars(&make_filename(&filepath));

        // create sha256, sha1, md5 and ssddeep hashes
        let md5output = make_md5(&filepath);
        let sha1output = make_sha1(&filepath);
        let sha256output = make_sha256(&filepath);
        let ssdeepoutput = make_ssdeep(&filepath);

        // XXX DEBUG

        // put tsv output to STDOUT
        match create_tsv(inputname, md5output, sha1output, sha256output, ssdeepoutput, context) {
            Err(e) => eprintln!("{:?}", e),
            _ => ()
        }

    }

}


fn create_tsv(
    inputname: String,
    inputmd5: String,
    inputsha1: String,
    inputsha256: String,
    inputssdeep: String,
    inputcontext: Vec<String>,
    ) -> Result<(), Box<dyn Error>> {

    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .quote_style(csv::QuoteStyle::Always)
        .from_writer(io::stdout());
    match wtr.write_record(&[inputname, inputmd5, inputsha1, inputsha256, inputssdeep, inputcontext.join(",")]){
        Err(e) => eprintln!("{:?}", e),
        _ => ()
    }

    wtr.flush()?;
    Ok(())
}


/// Check whether we are given a file
/// input: Path
/// output: continue or exit(1)
fn check_file(filename: &String) {
    let filepath = Path::new(&filename);
    if filepath.is_file() {
        eprintln!("processing {}", &filename);
    } else {
        eprintln!("{} is not a file", &filename);
        std::process::exit(0x0001);
    }
}

/// Replace all unwanted characters from input with '_'
/// input: &String [unclean input]
/// output: &String [removed badchars]
fn remove_badchars(inputstring: &String) -> String {
    // https://programming-idioms.org/idiom/147/remove-all-non-ascii-characters
    // it seems is_control removes both ascii and utf8 control chars
    // let noasciicontrol = inputstring.replace(|c: char| c.is_ascii_control(), "");
    let noutfcontrol = inputstring.replace(|c: char| c.is_control(), "");
    // https://users.rust-lang.org/t/fast-removing-chars-from-string/24554
    // is_alphanumberic removes punctuation chars we like, so a blocklist it is.
    // Two characters are very important to clean out: "|" (used for context-strings)
    // and "/" (used to combine primary context strings). 
    // XXX don't think the above is true any more
    let nobadchars = noutfcontrol.replace(
        &[
        '�', '|', '/', '{', '}', ':', '\\', '(', ')', ',', '\"', ' ', ';', '\'',
        ][..],
        "",
        ).to_string();
    format!("{}", nobadchars)
}

/// Turn the filename into a string for parsing
/// input: &string [full path]
/// output: String [filename as lossy string]
fn make_filename(filename: &String) -> String {
    let filepath = Path::new(&filename);
    let newfilename = match filepath.file_name() {
        Some(newfilename) => newfilename,
        None => panic!("Cannot get a filename"),
    };
    let str_newfilename = String::from(newfilename.to_string_lossy());
    str_newfilename
}

/// Get the md5 of a file reference
/// input: &String [full path to file]
/// output: String [md5 hash]
fn make_md5(filepath: &String) -> String {
    let mut hasher = Md5::new();
    // Also works on a dir, so might need to verify its a "regular" file
    // This seems to need to be a mut, otherwise io::copy has mixed types
    let mut file = fs::File::open(&filepath).expect("Unable to open file");
    // placeholder (unused) _variable
    let _bytes_written = io::copy(&mut file, &mut hasher);
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

/// Get the sha1 of a file reference
/// input: &String [full path to file]
/// output: String [sha1 hash]
fn make_sha1(filepath: &String) -> String {
    let mut hasher = Sha1::new();
    let mut file = fs::File::open(&filepath).expect("Unable to open file");
    let _bytes_written = io::copy(&mut file, &mut hasher);
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

/// Get the sha256 of a file reference
/// input: &String [full path to file]
/// output: String [sha256 hash]
fn make_sha256(filepath: &String) -> String {
    let mut hasher = Sha256::new();
    let mut file = fs::File::open(&filepath).expect("Unable to open file");
    let _bytes_written = io::copy(&mut file, &mut hasher);
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

/// Get the ssdeep of a file
/// input: &String [full path to string]
/// output: String [ssdeep hash]
fn make_ssdeep(filepath: &String) -> String {
    let hash = ssdeep::hash_from_file(&filepath).unwrap();
    hash
}

/// Split provided context on comma and return a vec
/// input: &String from args.context
/// output: Vec<String> (*cleaned* list of contexts)
fn make_context(context: &String) -> Vec<String> {
    let context_vec: Vec<String> = context
        .split(",")
        .map(|s| remove_badchars(&s.to_string()))
        .collect();
    context_vec
}

/// Connect to redis
/// input: host ip address, redis db number and redis password
/// output: redis::Connection
fn connect(redispassword: String, redishost: IpAddr, redisport: i32, redisdb: i32) -> Connection {
    redishost.to_string();
    redisdb.to_string();

    Client::open(format!(
            "redis://:{}@{}:{}/{}",
            redispassword, redishost, redisport, redisdb
            ))
        .expect("invalid connection URL")
        .get_connection()
        .expect("Failed to connect to Redis")
}

/// Make timestamp to track latest additions, mark chaches etc.
/// input: None
/// output: String [epoch.as_micros]
fn make_timestamp() -> u128 {
    let since_the_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    // return format!("{:?}", since_the_epoch.as_micros());
    let timestamp = since_the_epoch.as_micros();
    timestamp
}

fn set_timestamp(con: &mut Connection, timestamp: u128) -> RedisResult<()> {
    let set_timestamp = con.set("timestamp", timestamp.to_string());
    set_timestamp
}

/// Get current timestamp in redis
/// input: redis::Connection
/// output: RedisResult<usize> (timestamp)
// DEBUG
fn get_timestamp(con: &mut Connection) -> RedisResult<u128> {
   let timestamp = con.get("timestamp");
   timestamp
}

/// Create a list of original ssdeep and all its rolling windows
/// input: String (ssdeep string>
/// output: Vec<String> containing original ssdeep at [0] and rolling windows, both block sizes
fn make_rolling_windows(ssdeep_hash: &String) -> Vec<String> {
    // https://stackoverflow.com/questions/26643688/how-do-i-split-a-string-in-rust
    let ssdeep_parts: Vec<String> = ssdeep_hash.split(":").map(|s| s.to_string()).collect();
    let blocksizestring = &ssdeep_parts[0];
    let blocksize: i32 = blocksizestring
        .parse()
        .expect("blocksize should always be an int");
    let ssdeep_part_single = &ssdeep_parts[1];
    let ssdeep_part_double = &ssdeep_parts[2];
    let blocksize_double = &blocksize * 2;

    let mut rolling_window_vec: Vec<String> = Vec::new();
    // XXX creates an sset with value as key = ssdeep :/
    // rolling_window_vec.push(ssdeep_hash.to_string());
    rolling_window_vec.extend(get_all_7_char_rolling_window(
            &blocksize,
            &remove_plusthree_chars(ssdeep_part_single),
            &blocksize_double,
            &remove_plusthree_chars(ssdeep_part_double),
            ));
    rolling_window_vec
}

/// The function below removes all 4 consecutive chars, until 3 consecutive chars are left
/// The ssdeep algorithm does this internally for ssdeep_compare too.
/// input: ssdeep partial string (single or double)
/// output: ssdeep partial string reduced
fn remove_plusthree_chars(ssdeep_part: &String) -> String {
    let mut ssdeep_clean: String = ssdeep_part.to_string();
    let chars: Vec<char> = ssdeep_part.chars().collect();
    for c in chars {
        let c4: String = [c, c, c, c].iter().collect();
        let c3: String = [c, c, c].iter().collect();
        ssdeep_clean = ssdeep_clean.replace(&c4, &c3);
    }
    ssdeep_clean
}

/// Create Vec from preprocessedssdeep containing single & double <blocksize>:<7 char rolling windows>
/// input: blocksize<i32>, blockdata<String>, blocksize_double<i32>, blockdata_double<String>
/// output: Vec<String>
fn get_all_7_char_rolling_window(
    blocksize: &i32,
    blockdata: &String,
    blocksize_double: &i32,
    blockdata_double: &String,
    ) -> Vec<String> {
    let blockdata_as_vec: Vec<char> = blockdata.chars().collect();
    let blockdata_double_as_vec: Vec<char> = blockdata_double.chars().collect();
    let mut rolling_window_vec: Vec<String> = Vec::new();
    for window in blockdata_as_vec.windows(7) {
        let window_string: String = window.iter().collect();
        rolling_window_vec.push(format!("{}:{}", blocksize, window_string));
    }
    for window in blockdata_double_as_vec.windows(7) {
        let window_string: String = window.iter().collect();
        rolling_window_vec.push(format!("{}:{}", blocksize_double, window_string));
    }
    rolling_window_vec
}

/// create a Vec of unique ssdeeps as found under the rolling windows
/// input: Vec<String> of original ssdeep and rolling windows
/// output: a Vec<string> of all similar ssdeeps
fn get_similar_ssdeep_sets(
    // in kathe-cli.py, this is get_ssdeep_sets
    con: &mut Connection,
    original_ssdeep: &String,
    rolling_windows_ssdeep: &Vec<String>,
    ) -> HashSet<String> {
    let mut ssdeep_siblings = HashSet::new();
    for rolling_window_ssdeep in rolling_windows_ssdeep {
        let siblings: Vec<String> = con.smembers(rolling_window_ssdeep).unwrap();
        for sibling in siblings {
            ssdeep_siblings.insert(sibling);
        }
    }
    ssdeep_siblings.remove(original_ssdeep);
    ssdeep_siblings
}

/// store inputssdeep string under rolling_window_ssdeep key (unsorted unique set)
/// If a key does not exist, it is created
/// input: rolling_window_ssdeep
/// output: None/RedisResult<()>
/// stored: SMEMBER <rolling_window>,<ssdeep>
fn add_ssdeep_to_rolling_window(
    con: &mut Connection,
    rolling_window_ssdeep: &String,
    inputssdeep: &String,
    ) -> () {
    let _: RedisResult<()> = con.sadd(rolling_window_ssdeep, inputssdeep);
}

/// This function will store our info into redis
/// The four info fields contain a set (read: unique) of information
/// about the added entity. This way sha256/inputname/inputssdeep are
/// linked and retrievable.
/// XXX This will be done differently, with SortedSet keys, and only the
/// ssdeep:<ssdeep> key containing all the details using zincr
/// zincr(key, member, delta)
fn add_data(
    con: &mut Connection,
    inputname: &String,
    inputmd5: &String,
    inputsha1: &String,
    inputsha256: &String,
    inputssdeep: &String,
    inputcontext: &Vec<String>,
    ) -> RedisResult<()> {
    let ssdeep_rolling_window = make_rolling_windows(&inputssdeep);
    for rolling_window in ssdeep_rolling_window.iter() {
        add_ssdeep_to_rolling_window(con, rolling_window, &inputssdeep);
    }
    // here we add similar ssdeeps to the input ssdeep and vice versa
    let similar_ssdeeps = get_similar_ssdeep_sets(con, &inputssdeep, &ssdeep_rolling_window);
    /// if there is no similar_ssdeep, no key is created. Which is similar to Redis behaviour:
    /// empty zsets are not created.
    for similar_ssdeep in similar_ssdeeps.iter() {
        // This will never be 0/none, because of the window overlap
        let score = ssdeep::compare(&inputssdeep.as_bytes(), &similar_ssdeep.as_bytes()).unwrap();
        let _: RedisResult<()> = con.zadd(
            format!("{}", inputssdeep),
            format!("{}", similar_ssdeep),
            score,
            );
        let _: RedisResult<()> = con.zadd(
            format!("{}", similar_ssdeep),
            format!("{}", inputssdeep),
            score,
            );
    }

    let _: RedisResult<()> = con.zincr(
        format!("ssdeep:{}", inputssdeep),
        format!("inputname:{}", inputname),
        1,
        );

    let _: RedisResult<()> = con.zincr(
        format!("ssdeep:{}", inputssdeep),
        format!("md5:{}", inputmd5),
        1,
        );
    let _: RedisResult<()> = con.zincr(
        format!("ssdeep:{}", inputssdeep),
        format!("sha1:{}", inputsha1),
        1,
        );
    let _: RedisResult<()> = con.zincr(
        format!("ssdeep:{}", inputssdeep),
        format!("sha256:{}", inputsha256),
        1,
        );

    // the unwrap() feels like a hack to bypass type mismatch
    for context in inputcontext.iter() {
        match context {
            _ => con
                .zincr(
                    format!("ssdeep:{}", inputssdeep),
                    format!("context:{}", context),
                    1,
                    )
                .unwrap(),
        }
    }
    for context in inputcontext.iter() {
        match context {
            _ => con
                .zincr(
                    format!("context:{}", context),
                    format!("{}", inputssdeep),
                    1,
                    )
                .unwrap(),
        }
    }
    let _: RedisResult<()> = con.zincr(
        format!("inputname:{}", inputname),
        format!("{}", inputssdeep),
        1,
        );

    let _: RedisResult<()> = con.zincr(format!("md5:{}", inputmd5), format!("{}", inputssdeep), 1);
    let _: RedisResult<()> =
        con.zincr(format!("sha1:{}", inputsha1), format!("{}", inputssdeep), 1);

    let _: RedisResult<()> = con.zincr(
        format!("sha256:{}", inputsha256),
        format!("{}", inputssdeep),
        1,
        );

    // build indexes
    let _: RedisResult<()> = con.zincr("index:inputname", &inputname, 1);
    let _: RedisResult<()> = con.zincr("index:ssdeep", &inputssdeep, 1);
    let _: RedisResult<()> = con.zincr("index:md5", &inputmd5, 1);
    let _: RedisResult<()> = con.zincr("index:sha1", &inputsha1, 1);
    let _: RedisResult<()> = con.zincr("index:sha256", &inputsha256, 1);

    for context in inputcontext.iter() {
        let _: RedisResult<()> = con.zincr("index:context", context, 1);
    }
    // timestamp dance
    let inputtimestamp = make_timestamp();

    //let old_timestamp = match get_timestamp(con) {
    //    Ok(ts) => ts,
    //    Err(_) => inputtimestamp,
    //};

    let _ = set_timestamp(con, inputtimestamp);

    // I don't think I care about primary
    // XXX DEBUG OUTPUT
    // println!(
    //    "old_timestamp: {}, timestamp: {}, inputname: {}, inputmd5: {} ,inputsha1: {}, inputsha256: {}, inputssdeep: {:#?}, inputcontext: {:#?}, ssdeep_rolling_window: {:#?}, similar_ssdeeps: {:#?} ",
    //    old_timestamp,
    //    inputtimestamp,
    //    inputname,
    //    inputmd5,
    //    inputsha1,
    //    inputsha256,
    //    inputssdeep,
    //   inputcontext,
    //   ssdeep_rolling_window,
    //   similar_ssdeeps,
    //);
    // XXX DEBUG OUTPUT END

    Ok(())
}
