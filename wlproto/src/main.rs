mod iter_ext;


use std::path::PathBuf;

use clap::Parser;
use sxd_document::QName;
use sxd_document::dom::Element;

use crate::iter_ext::SingleIterExt;


#[derive(Parser)]
struct Opts {
    pub xml_proto_input: PathBuf,
    pub rust_output: PathBuf,
}


fn main() {
    let opts = Opts::parse();

    let xml_proto_string = std::fs::read_to_string(&opts.xml_proto_input)
        .expect("failed to read protocol XML");
    let proto_package = sxd_document::parser::parse(&xml_proto_string)
        .expect("failed to parse protocol XML");
    let root_elem = proto_package
        .as_document()
        .root()
        .children()
        .into_iter()
        .filter_map(|n| n.element())
        .single().expect("multiple root elements");
    if root_elem.name() != QName::new("protocol") {
        panic!("root element is not <protocol>");
    }
    let proto_name = root_elem.attribute_value("name")
        .expect("root <protocol> is missing name=\"...\"");

    let proto_child_elems = root_elem
        .children()
        .into_iter()
        .filter_map(|n| n.element());
    for proto_child_elem in proto_child_elems {
        if proto_child_elem.name() == QName::new("copyright") {
            // your interface XML probably falls under a software interoperability exception
            // and you therefore cannot exercise copyright over it
            // and if you have chosen a restrictive license, I laugh at you
            continue;
        } else if proto_child_elem.name() == QName::new("interface") {
            process_interface(proto_child_elem);
        }
    }
}

fn process_interface(interface_elem: Element<'_>) {
    todo!();
}
