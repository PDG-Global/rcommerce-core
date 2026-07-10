#!/bin/bash

# Payment Gateway Integration Test Runner
#
# Usage:
#   ./run_payment_tests.sh              # Check env vars
#   ./run_payment_tests.sh stripe       # Run Stripe tests only
#   ./run_payment_tests.sh airwallex    # Run Airwallex tests only
#   ./run_payment_tests.sh wechatpay    # Run WeChat Pay tests only
#   ./run_payment_tests.sh all          # Run all tests

set -e

cd "$(dirname "$0")/../.."

echo "🔧 R Commerce Payment Gateway Integration Tests"
echo "================================================"
echo ""

# Function to check if environment variable is set
check_env() {
    if [ -z "${!1}" ]; then
        echo "❌ $1 is not set"
        return 1
    else
        echo "✅ $1 is set"
        return 0
    fi
}

# Check environment variables
echo "📋 Checking environment variables..."
echo ""

STRIPE_READY=true
AIRWALLEX_READY=true
WECHATPAY_READY=true

if ! check_env "STRIPE_TEST_SECRET_KEY"; then
    STRIPE_READY=false
fi

if ! check_env "STRIPE_TEST_WEBHOOK_SECRET"; then
    STRIPE_READY=false
fi

if ! check_env "AIRWALLEX_TEST_CLIENT_ID"; then
    AIRWALLEX_READY=false
fi

if ! check_env "AIRWALLEX_TEST_API_KEY"; then
    AIRWALLEX_READY=false
fi

if ! check_env "AIRWALLEX_TEST_WEBHOOK_SECRET"; then
    AIRWALLEX_READY=false
fi

if ! check_env "WECHATPAY_MCH_ID"; then
    WECHATPAY_READY=false
fi

if ! check_env "WECHATPAY_APP_ID"; then
    WECHATPAY_READY=false
fi

if ! check_env "WECHATPAY_SERIAL_NO"; then
    WECHATPAY_READY=false
fi

if ! check_env "WECHATPAY_PRIVATE_KEY"; then
    WECHATPAY_READY=false
fi

echo ""

# Show usage if no arguments
if [ $# -eq 0 ]; then
    echo "Usage:"
    echo "  $0 check      - Check environment variables"
    echo "  $0 stripe     - Run Stripe integration tests"
    echo "  $0 airwallex  - Run Airwallex integration tests"
    echo "  $0 wechatpay  - Run WeChat Pay integration tests"
    echo "  $0 all        - Run all integration tests"
    echo ""
    echo "Environment variables needed:"
    echo ""
    echo "Stripe:"
    echo "  STRIPE_TEST_SECRET_KEY"
    echo "  STRIPE_TEST_WEBHOOK_SECRET"
    echo ""
    echo "Airwallex:"
    echo "  AIRWALLEX_TEST_CLIENT_ID"
    echo "  AIRWALLEX_TEST_API_KEY"
    echo "  AIRWALLEX_TEST_WEBHOOK_SECRET"
    echo ""
    echo "WeChat Pay:"
    echo "  WECHATPAY_MCH_ID"
    echo "  WECHATPAY_APP_ID"
    echo "  WECHATPAY_SERIAL_NO"
    echo "  WECHATPAY_PRIVATE_KEY"
    echo "  WECHATPAY_API_KEY"
    exit 0
fi

COMMAND=$1

case $COMMAND in
    check)
        echo "🔍 Environment check complete"
        if [ "$STRIPE_READY" = true ]; then
            echo "   Stripe tests: READY"
        else
            echo "   Stripe tests: MISSING CREDENTIALS"
        fi
        
        if [ "$AIRWALLEX_READY" = true ]; then
            echo "   Airwallex tests: READY"
        else
            echo "   Airwallex tests: MISSING CREDENTIALS"
        fi
        
        if [ "$WECHATPAY_READY" = true ]; then
            echo "   WeChat Pay tests: READY"
        else
            echo "   WeChat Pay tests: MISSING CREDENTIALS"
        fi
        ;;
    
    stripe)
        if [ "$STRIPE_READY" = false ]; then
            echo "❌ Cannot run Stripe tests - missing environment variables"
            echo ""
            echo "Set the following variables:"
            echo "  export STRIPE_TEST_SECRET_KEY=\"sk_test_...\""
            echo "  export STRIPE_TEST_WEBHOOK_SECRET=\"whsec_...\""
            exit 1
        fi
        
        echo "🚀 Running Stripe integration tests..."
        echo ""
        cargo test --test payment_integration_tests stripe -- --test-threads=1 --ignored
        ;;
    
    airwallex)
        if [ "$AIRWALLEX_READY" = false ]; then
            echo "❌ Cannot run Airwallex tests - missing environment variables"
            echo ""
            echo "Set the following variables:"
            echo "  export AIRWALLEX_TEST_CLIENT_ID=\"...\""
            echo "  export AIRWALLEX_TEST_API_KEY=\"...\""
            echo "  export AIRWALLEX_TEST_WEBHOOK_SECRET=\"...\""
            exit 1
        fi
        
        echo "🚀 Running Airwallex integration tests..."
        echo ""
        cargo test --test payment_integration_tests airwallex -- --test-threads=1 --ignored
        ;;
    
    wechatpay)
        if [ "$WECHATPAY_READY" = false ]; then
            echo "❌ Cannot run WeChat Pay tests - missing environment variables"
            echo ""
            echo "Set the following variables:"
            echo "  export WECHATPAY_MCH_ID=\"1234567890\""
            echo "  export WECHATPAY_APP_ID=\"wx1234567890abcdef\""
            echo "  export WECHATPAY_SERIAL_NO=\"...\""
            echo "  export WECHATPAY_PRIVATE_KEY=\"-----BEGIN PRIVATE KEY-----...\""
            echo "  export WECHATPAY_API_KEY=\"...\""
            exit 1
        fi
        
        echo "🚀 Running WeChat Pay integration tests..."
        echo ""
        cargo test --test wechatpay_integration_tests -- --test-threads=1 --ignored
        ;;
    
    all)
        echo "🚀 Running all payment integration tests..."
        echo ""
        
        if [ "$STRIPE_READY" = true ]; then
            echo "Running Stripe tests..."
            cargo test --test payment_integration_tests stripe -- --test-threads=1 --ignored
        else
            echo "⚠️ Skipping Stripe tests (missing credentials)"
        fi
        
        if [ "$AIRWALLEX_READY" = true ]; then
            echo "Running Airwallex tests..."
            cargo test --test payment_integration_tests airwallex -- --test-threads=1 --ignored
        else
            echo "⚠️ Skipping Airwallex tests (missing credentials)"
        fi
        
        if [ "$WECHATPAY_READY" = true ]; then
            echo "Running WeChat Pay tests..."
            cargo test --test wechatpay_integration_tests -- --test-threads=1 --ignored
        else
            echo "⚠️ Skipping WeChat Pay tests (missing credentials)"
        fi
        ;;
    
    *)
        echo "❌ Unknown command: $COMMAND"
        echo ""
        echo "Valid commands: check, stripe, airwallex, wechatpay, all"
        exit 1
        ;;
esac

echo ""
echo "✅ Test run complete!"
