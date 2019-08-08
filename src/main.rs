#[macro_use]
extern crate json;
extern crate term;
extern crate dirs;
extern crate crypto;
extern crate urlencoding;
extern crate rust_util;
#[macro_use]
extern crate lazy_static;

pub mod jdk;
pub mod local_util;
pub mod http;
pub mod tool;
pub mod build_json;
pub mod misc;

use std::{
    fs,
    process::Command,
};

use rust_util::{
    util_msg::{
        print_message,
        MessageType,
    },
    util_cmd::run_command_and_wait,
};
use tool::*;
use jdk::*;
use build_json::*;
use misc::*;

const BUILDJ: &str = "buildj";
const BUDERJ_VER: &str = env!("CARGO_PKG_VERSION");
const GIT_HASH: &str = env!("GIT_HASH");
const BUILD_DATE: &str = env!("BUILD_DATE");


fn do_with_buildin_arg_java(first_arg: &str, args: &Vec<String>) {
    let ver = &first_arg[7..];
    if ver == "" {
        print_message(MessageType::ERROR, &format!("Java version is not assigned!"));
        return;
    }
    match get_java_home(ver) {
        None => print_message(MessageType::ERROR, &format!("Assigned java version not found: {}", ver)),
        Some(java_home) => {
            print_message(MessageType::OK, &format!("Find java home: {}", java_home));
            let java_bin = &format!("{}/bin/java", java_home);
            let mut cmd = Command::new(java_bin);
            cmd.envs(&get_env_with_java_home(&java_home));
            if args.len() > 2 {
                cmd.args(&args[2..]);
            }
            run_command_and_wait(&mut cmd).unwrap_or_else(|err| {
                print_message(MessageType::ERROR, &format!("Exec java failed: {}", err));
            });
        },
    };
}

fn do_with_buildin_arg_maven(first_arg: &str, args: &Vec<String>) {
    do_with_buildin_arg_builder(first_arg, args, "maven", "MAVEN_HOME", "mvn")
}

fn do_with_buildin_arg_gradle(first_arg: &str, args: &Vec<String>) {
    do_with_buildin_arg_builder(first_arg, args, "gradle", "GRADLE_HOME", "gradle")
}

fn do_with_buildin_arg_builder(first_arg: &str, args: &Vec<String>, builder_name: &str, builder_home: &str, builder_bin: &str) {
    let builder_version = &first_arg[(builder_name.len() + 3)..];
    if builder_version == "" {
        print_message(MessageType::ERROR, &format!("Builder version is not assigned!"));
        return;
    }
    let mut has_java = false;
    let mut java_home = String::new();
    if args.len() > 2 && args[2].starts_with("--java") {
        has_java = true;
        let java_version = &args[2][6..];
        if java_version != "" {
            java_home = match get_java_home(java_version) {
                None => {
                    print_message(MessageType::ERROR, &format!("Assigned java version not found: {}", java_version));
                    return;
                },
                Some(h) => h,
            };
        }
    }
    let builder_desc = match tool::get_builder_home(builder_name, builder_version) {
        None => {
            print_message(MessageType::ERROR, &format!("Assigned builder: {}, version: {} not found.", builder_name, builder_version));
            return;
        },
        Some(h) => h,
    };
    if has_java {
        print_message(MessageType::OK, &format!("JAVA_HOME    = {}", java_home));
    }
    print_message(MessageType::OK, &format!("BUILDER_HOME = {}", &builder_desc.home));

    let mut new_env = match has_java {
        true => get_env_with_java_home(&java_home),
        false => get_env(),
    };
    new_env.insert(builder_home.to_string(), builder_desc.home.clone());

    let mut cmd = Command::new(format!("{}/bin/{}", builder_desc.home.clone(), builder_bin));
    cmd.envs(&new_env);
    let from_index = match has_java { true => 3, false => 2 };
    for i in from_index..args.len() {
        cmd.arg(&args[i]);
    }
    run_command_and_wait(&mut cmd).unwrap_or_else(|err| {
        print_message(MessageType::ERROR, &format!("Run build command failed: {}", err));
    });
}

fn do_with_buildin_args(args: &Vec<String>) {
     let first_arg = args.get(1).unwrap();
    if first_arg == ":::" || first_arg == ":::help" {
        print_usage();
    } else if first_arg == ":::create" {
        create_build_json(&args);
    } else if first_arg.starts_with(":::java") {
        do_with_buildin_arg_java(first_arg, args);
    } else if first_arg.starts_with(":::maven") {
        do_with_buildin_arg_maven(first_arg, args);
    } else if first_arg.starts_with(":::gradle") {
        do_with_buildin_arg_gradle(first_arg, args);
    } else {
        print_message(MessageType::ERROR, &format!("Unknown args: {:?}", &args));
    }
}


fn main() {
    print_message(MessageType::INFO, &format!("{} - version {} - {}", BUILDJ, BUDERJ_VER, &GIT_HASH[0..7]));

    if *VERBOSE {
        print_message(MessageType::DEBUG, &format!("Full GIT_HASH: {}", GIT_HASH));
        print_message(MessageType::DEBUG, &format!("Build date: {}", BUILD_DATE));
    }

    let args = local_util::get_args_as_vec();
    print_message(MessageType::INFO, &format!("Arguments: {:?}", args));

    if local_util::is_buildin_args(&args) {
        do_with_buildin_args(&args);
        return;
    }
    local_util::init_home_dir(tool::LOCAL_BUILDER_HOME_BASE_DIR);
    local_util::init_home_dir(jdk::LOCAL_JAVA_HOME_BASE_DIR);

    let build_json = match find_build_json() {
        None => return,
        Some(p) => p,
    };

    print_message(MessageType::OK, &format!("Find {} @ {}", BUILD_JSON, build_json));

    let build_json_content = match fs::read_to_string(build_json) {
        Err(err) => {
            print_message(MessageType::ERROR, &format!("Read {} failed: {}", BUILD_JSON, err));
            return;
        },
        Ok(content) => content,
    };
    let build_json_object = match json::parse(&build_json_content) {
        Err(err) => {
            print_message(MessageType::ERROR, &format!("Parse JSON failed: {}", err));
            return;
        },
        Ok(object) => object,
    };

    let java_version_j = &build_json_object["java"];
    let builder_name_j = &build_json_object["builder"]["name"];
    let builder_version_j = &build_json_object["builder"]["version"];
    if java_version_j.is_null() {
        print_message(MessageType::ERROR, "Java version is not assigned!");
        return;
    }
    if builder_name_j.is_null() || builder_version_j.is_null() {
        print_message(MessageType::ERROR, "Builder name or version is not assigned!");
        return;
    }
    let java_version = java_version_j.as_str().unwrap();
    let builder_name = builder_name_j.as_str().unwrap();
    let builder_version = builder_version_j.as_str().unwrap();

    let java_home = match get_java_home(java_version) {
        None => {
            print_message(MessageType::ERROR, &format!("Assigned java version not found: {}", java_version));
            return;
        },
        Some(h) => h,
    };
    let builder_desc = match tool::get_builder_home(builder_name, builder_version) {
        None => {
            print_message(MessageType::ERROR, &format!("Assigned builder: {}, version: {} not found.", builder_name, builder_version));
            return;
        },
        Some(h) => h,
    };
    print_message(MessageType::OK, &format!("JAVA_HOME    = {}", java_home));
    print_message(MessageType::OK, &format!("BUILDER_HOME = {}", &builder_desc.home));

    let mut new_env = get_env_with_java_home(&java_home);
    let builder_home_env = match builder_desc.name { BuilderName::Maven => "MAVEN_HOME", BuilderName::Gradle => "GRADLE_HOME", };
    new_env.insert(builder_home_env.to_string(), builder_desc.home.clone());
   
    let cmd_bin = match builder_desc.name {
        BuilderName::Maven => builder_desc.bin.unwrap_or(format!("{}/bin/mvn", builder_desc.home.clone())),
        BuilderName::Gradle => builder_desc.bin.unwrap_or(format!("{}/bin/gradle", builder_desc.home.clone())),
    };

    let mut cmd = Command::new(cmd_bin);
    cmd.envs(&new_env);
    for i in 1..args.len() {
        cmd.arg(&args[i]);
    }
    run_command_and_wait(&mut cmd).unwrap_or_else(|err| {
        print_message(MessageType::ERROR, &format!("Run build command failed: {}", err));
    });
}