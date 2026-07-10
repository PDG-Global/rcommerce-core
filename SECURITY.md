# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.x.x   | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a Vulnerability

We take security seriously. If you discover a security vulnerability in R
Commerce, please report it responsibly.

### How to Report

**DO NOT** create a public GitHub issue for security vulnerabilities.

Instead, please email us at:

 **security@rcommerce.app**

Please include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)
- Your contact information for follow-up

### Response Timeline

| Timeframe | Action |
|-----------|--------|
| Within 24 hours | Acknowledgment of receipt |
| Within 72 hours | Initial assessment |
| Within 7 days | Fix developed and tested |
| Within 14 days | Security patch released |

### What to Expect

1. **Confirmation**: We'll confirm receipt within 24 hours
2. **Assessment**: We'll evaluate the severity and impact
3. **Fix Development**: We'll develop and test a fix
4. **Disclosure**: We'll coordinate disclosure timeline with you
5. **Credit**: We'll credit you in the security advisory (if desired)

## Security Best Practices

### For Users

1. **Keep Updated**: Always use the latest version
2. **Secure Configuration**:
   - Change default passwords
   - Use strong JWT secrets
   - Enable rate limiting
   - Use TLS/SSL in production

3. **Environment Security**:
   - Don't commit secrets to version control
   - Use environment variables for sensitive data
   - Restrict database access
   - Use network segmentation

4. **Monitoring**:
   - Enable audit logging
   - Monitor for suspicious activity
   - Set up alerts for anomalies

### For Developers

1. **Input Validation**: Always validate and sanitize user input
2. **Parameterized Queries**: Use SQLx to prevent SQL injection
3. **Authentication**: Verify all protected endpoints
4. **Authorization**: Check permissions for each action
5. **Dependencies**: Keep dependencies updated
6. **Secrets**: Never log or expose secrets

## Security Features

R Commerce includes several built-in security features:

- **Rate Limiting**: Configurable per-IP and per-API-key limits
- **JWT Authentication**: Secure token-based authentication
- **API Key Management**: Scoped API keys with rotation support
- **Audit Logging**: Comprehensive audit trail
- **Input Validation**: Built-in validation using `validator`
- **SQL Injection Protection**: Parameterized queries via SQLx
- **CORS Configuration**: Configurable cross-origin policies

## Known Security Considerations

### Current Limitations

1. **Self-Hosted**: Security depends on proper deployment configuration
2. **API Keys**: Store API keys securely; they provide full access
3. **Database**: Database security is the operator's responsibility

### Recommendations

- Use a Web Application Firewall (WAF) in production
- Implement DDoS protection at the network level
- Regular security audits and penetration testing
- Follow the principle of least privilege

## Security Advisories

Security advisories will be published on:
- GitHub Security Advisories
- Our security mailing list
- The R Commerce blog

## Compliance

R Commerce is designed to help with compliance for:
- PCI DSS (when properly configured)
- GDPR (data handling capabilities)
- SOC 2 (audit logging and access controls)

Note: Compliance is a shared responsibility. Proper configuration and
operational practices are required.

## Contact

- **Security Issues**: security@rcommerce.app
- **General Questions**: dev@rcommerce.app
- **Emergency**: +1-XXX-XXX-XXXX (24/7 hotline)

---

Thank you for helping keep R Commerce secure!
