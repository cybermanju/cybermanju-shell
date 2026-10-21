// SPDX-License-Identifier: GPL-2.0-only
/* Real kbuild fixture. Build/inspect only; never load it into the host kernel. */
#include <linux/init.h>
#include <linux/module.h>

static int __init cybermanju_fixture_init(void)
{
    return 0;
}

static void __exit cybermanju_fixture_exit(void)
{
}

module_init(cybermanju_fixture_init);
module_exit(cybermanju_fixture_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("CyberManju ELF inspection fixture; not native loader evidence");
MODULE_VERSION("0.1.0");
