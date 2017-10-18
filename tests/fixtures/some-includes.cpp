// License comment or something

#include<stdio.h>
#	include <cstdlib>
#include "whatever.h"

int main(int argc, char* argv[])
{
  #include <something>
  return 0;
  #include not even the right syntax, but it'll still get removed
}
