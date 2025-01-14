#!/bin/bash
set -e

# You can run it from any directory.
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

TEST_COUNTER=1
TEST_RUN_SUCCESS="false"

function printTestResults {
	echo ""
	if [ "$TEST_RUN_SUCCESS" == "true" ]; then
		echo "Test run SUCCESS, $TEST_COUNTER test(s)."
	else
		echo "Test run FAILED, $TEST_COUNTER test(s)."
		echo "To log each step: export DEBUG_MODE_FOR_ALL_TESTS=true"
	fi
}

# Hook to exit happened either because of success or error.
trap printTestResults EXIT

pushd "$DIR/../" > /dev/null

"$DIR/build_and_unit_tests.sh"
TEST_COUNTER=$((TEST_COUNTER+1))

"$DIR/clippy.sh"
TEST_COUNTER=$((TEST_COUNTER+1))

popd > /dev/null

TEST_RUN_SUCCESS="true"
