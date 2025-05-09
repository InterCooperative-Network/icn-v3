module.exports = {
  extends: ['@commitlint/config-conventional'],
  rules: {
    'body-max-line-length': [2, 'always', 100],
    'scope-enum': [
      2,
      'always',
      [
        'repo',
        'ci',
        'docs',
        'types',
        'crypto',
        'identity',
        'covm',
        'ccl',
        'agoranet',
        'dag',
        'p2p',
        'wallet',
        'tools',
        'services',
        'deps'
      ]
    ]
  }
}; 