# SMC Parser

This is a parser for the outputs of the Short Markov Chain (SMC) project that allows for the
translation between some of the possible outputs from our CLIs that are designed to
run predetermined SMC scripts and print out the (transposed) assignment matrix.

The CLI for this can be installed directly from git using cargo via the command

```
cargo install --git https://github.com/peterrrock2/smc_parser.git 
```

The main thing that this CLI does is enable the encoding of standard JSONL
outputs of the [redist](https://github.com/alarm-redist/redist) package
in the standardized JSONL format:


```
{"assignment": <assignment-vector>, "sample": <sample number>}
```

or in the [BEN](https://github.com/peterrrock2/binary-ensamble.git) format. 

## Usage

Here are a list of the flags for the CLI

- `-i --input-csv` (Optional) The CSV output of SMC. If not passed, it is assumed that the
  input is piped in from stdin

- `-o --output-file` (Optional) The name of the output file for the parsing. It is recommended
  that if you are parsing using the standard mode (without the `--ben` or `--jsonl` flag) that
  you include the appropriate file extension.

- `-j --jsonl` A boolean flag that, when included, indicates that the output should be written
  in the JSONL format.

- `-b --ben` A boolean flag that, when included, indicates that the output should be written
  in the BEN format.

- `-v --verbose` A boolean flag that, when included, will write some progress indicators to
  stderr

- `-w --overwrite` A boolean flag that, when included, will force the output file of the
  `-o` file to be overwritten and will suppress the user query prompt.


## Example

You can see the `smc_parser` at work by running the following command on the
example file:

```
smc_parser -i test_out_assignments.csv -j
```

(this assumes that `~/.cargo/bin/` is in your path and that you have installed the package).

In the event that you would like to replicate some of these outputs, you may run the provided
R cli tool using

```
Rscript ./smc_cli.R -s ./4x4_grid -p TOTPOP -n 4 --tally_cols x -o test_out.csv
```
and 
```
Rscript ./smc_cli.R -s ./4x4_grid -p TOTPOP -n 4 --tally_cols x --print > test_print_out.out 
```

the second command may then be chained with the SMC parser using

```
Rscript ./smc_cli.R -s ./4x4_grid -p TOTPOP -n 4 --tally_cols x --print 2> /dev/null | smc_parser -j
```

which will then produce the canonicalized JSONL output.