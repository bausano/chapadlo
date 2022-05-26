#!/bin/bash

#
# Runs unit tests and invokes the binary with a test file.
#
# We sort the lines of the output because the output is not deterministic in
# order of client ids. That the header is always the first line is asserted
# in unit tests.

cargo test || exit 1

test_file_1_output="$(cargo run -- test/assets/input1.csv | sort)"
expected_test_file_1_output="1,0.5000,3.0000,3.5000,false
2,3.0000,0.0000,3.0000,true
client,available,held,total,locked"
echo
echo
[[ "${expected_test_file_1_output}" == "${test_file_1_output}" ]] \
    && echo "✔ Test 1 one passed" \
    || echo -e "✘ Test 1 failed\n\n${test_file_1_output}"
echo

test_file_2_output="$(cargo run -- test/assets/input2.csv | sort)"
expected_test_file_2_output="1,1.5000,0.0000,1.5000,false
2,2.0000,0.0000,2.0000,false
client,available,held,total"
echo
echo
[[ "${expected_test_file_2_output}" == "${test_file_2_output}" ]] \
    && echo "✔ Test 2 one passed" \
    || echo -e "✘ Test 2 failed\n\n${test_file_2_output}"
echo
