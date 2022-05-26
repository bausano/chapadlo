#!/bin/bash

cargo test

test_file_1_output="$(cargo run -- test/assets/input1.csv | sort)"
expected_test_file_1_output="1,0.5000,3.0,3.5000,false
2,3.0,0.0,3.0,true
client,available,held,total,locked"

echo
echo
[[ "${expected_test_file_1_output}" == "${test_file_1_output}" ]] \
    && echo "✔ Test 1 one passed" \
    || echo "✘ Test 1 failed"
