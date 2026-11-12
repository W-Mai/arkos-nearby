//! Serialize input with the provenance captured by its Model.

use howff_menu::input::Sourced;
use serde::Serialize;
use std::io::{self, Write};

pub(crate) fn send<T: Serialize>(input: Sourced<T>) -> Result<(), String> {
    write(&mut io::stdout().lock(), input)
}

fn write<T: Serialize>(output: &mut impl Write, input: Sourced<T>) -> Result<(), String> {
    serde_json::to_writer(&mut *output, &input).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use howff_menu::input::Generation;

    #[test]
    fn serialized_actions_keep_the_model_origin_and_one_message_boundary() {
        let mut output = Vec::new();
        let input = Generation::FIRST.bind(crate::model::Action::Start);
        write(&mut output, input).unwrap();
        let decoded: Sourced<crate::model::Action> = serde_json::from_slice(&output).unwrap();
        assert_eq!(decoded.generation, Generation::FIRST);
        assert_eq!(decoded.payload, crate::model::Action::Start);
        assert_eq!(output.iter().filter(|byte| **byte == b'\n').count(), 1);
    }
}
